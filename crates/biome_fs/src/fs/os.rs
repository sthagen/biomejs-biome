//! Implementation of the [FileSystem] and related traits for the underlying OS filesystem

use std::env::temp_dir;
use std::fs::{FileType, Metadata};
use std::process::Command;
use std::{
    env, fs,
    io::{self, Read, Seek, Write},
    mem,
};

use biome_diagnostics::{DiagnosticExt, Error, IoError, Severity};
use camino::{Utf8DirEntry, Utf8Path, Utf8PathBuf};
use path_absolutize::Absolutize;
use rayon::{Scope, scope};
use tracing::instrument;

use crate::expand_symbolic_link;
use crate::fs::OpenOptions;
use crate::{
    BiomePath, FileSystem, MemoryFileSystem,
    fs::{TraversalContext, TraversalScope},
};

use super::{BoxedTraversal, File, FileSystemDiagnostic, FsErrorKind, PathKind};

/// Implementation of [FileSystem] that directly calls through to the underlying OS
pub struct OsFileSystem {
    pub working_directory: Option<Utf8PathBuf>,
}

impl OsFileSystem {
    pub fn new(working_directory: Utf8PathBuf) -> Self {
        Self {
            working_directory: Some(working_directory),
        }
    }
}

impl Default for OsFileSystem {
    fn default() -> Self {
        let working_directory = env::current_dir()
            .map(|p| Utf8PathBuf::from_path_buf(p).expect("To be a UTF-8 path"))
            .ok();
        Self { working_directory }
    }
}

impl FileSystem for OsFileSystem {
    fn open_with_options(
        &self,
        path: &Utf8Path,
        options: OpenOptions,
    ) -> io::Result<Box<dyn File>> {
        let mut fs_options = fs::File::options();
        Ok(Box::new(OsFile {
            inner: options.into_fs_options(&mut fs_options).open(path)?,
            version: 0,
        }))
    }

    fn traversal(&self, func: BoxedTraversal) {
        OsTraversalScope::with(move |scope| {
            func(scope);
        })
    }

    fn working_directory(&self) -> Option<Utf8PathBuf> {
        self.working_directory.clone()
    }

    fn path_exists(&self, path: &Utf8Path) -> bool {
        path.exists()
    }

    fn path_is_file(&self, path: &Utf8Path) -> bool {
        path.is_file()
    }

    fn path_is_dir(&self, path: &Utf8Path) -> bool {
        path.is_dir()
    }

    fn path_is_symlink(&self, path: &Utf8Path) -> bool {
        path.is_symlink()
    }

    fn path_kind(&self, path: &Utf8Path) -> Result<PathKind, FileSystemDiagnostic> {
        path_kind_from_metadata(path.metadata(), path)
    }

    fn symlink_path_kind(&self, path: &Utf8Path) -> Result<PathKind, FileSystemDiagnostic> {
        path_kind_from_metadata(path.symlink_metadata(), path)
    }

    fn read_link(&self, path: &Utf8Path) -> io::Result<Utf8PathBuf> {
        path.read_link_utf8()
    }

    fn get_changed_files(&self, base: &str) -> io::Result<Vec<String>> {
        let output = Command::new("git")
            .arg("diff")
            .arg("--name-only")
            .arg("--relative")
            // A: added
            // C: copied
            // M: modified
            // R: renamed
            // Source: https://git-scm.com/docs/git-diff#Documentation/git-diff.txt---diff-filterACDMRTUXB82308203
            .arg("--diff-filter=ACMR")
            .arg(format!("{base}...HEAD"))
            .output()?;

        Ok(String::from_utf8_lossy(&output.stdout)
            .lines()
            .map(|l| l.to_string())
            .collect())
    }

    fn get_staged_files(&self) -> io::Result<Vec<String>> {
        let output = Command::new("git")
            .arg("diff")
            .arg("--name-only")
            .arg("--relative")
            .arg("--staged")
            // A: added
            // C: copied
            // M: modified
            // R: renamed
            // Source: https://git-scm.com/docs/git-diff#Documentation/git-diff.txt---diff-filterACDMRTUXB82308203
            .arg("--diff-filter=ACMR")
            .output()?;

        Ok(String::from_utf8_lossy(&output.stdout)
            .lines()
            .map(|l| l.to_string())
            .collect())
    }
}

#[derive(Debug)]
struct OsFile {
    inner: fs::File,
    version: i32,
}

impl File for OsFile {
    fn read_to_string(&mut self, buffer: &mut String) -> io::Result<()> {
        // Reset the cursor to the starting position
        self.inner.rewind()?;
        // Read the file content
        self.inner.read_to_string(buffer)?;
        Ok(())
    }

    #[instrument(level = "debug")]
    fn set_content(&mut self, content: &[u8]) -> io::Result<()> {
        // Truncate the file
        self.inner.set_len(0)?;
        // Reset the cursor to the starting position
        self.inner.rewind()?;
        // Write the byte slice
        self.inner.write_all(content)?;
        // new version stored
        self.version += 1;
        Ok(())
    }

    fn file_version(&self) -> i32 {
        self.version
    }
}

#[repr(transparent)]
pub struct OsTraversalScope<'scope> {
    scope: Scope<'scope>,
}

impl<'scope> OsTraversalScope<'scope> {
    pub(crate) fn with<F>(func: F)
    where
        F: FnOnce(&Self) + Send,
    {
        scope(move |scope| func(Self::from_rayon(scope)))
    }

    fn from_rayon<'a>(scope: &'a Scope<'scope>) -> &'a Self {
        // SAFETY: transmuting from Scope to OsTraversalScope is safe since
        // OsTraversalScope has the `repr(transparent)` attribute that
        // guarantees its layout is the same as Scope
        unsafe { mem::transmute(scope) }
    }
}

impl<'scope> TraversalScope<'scope> for OsTraversalScope<'scope> {
    fn evaluate(&self, ctx: &'scope dyn TraversalContext, path: Utf8PathBuf) {
        // Path must be absolute in order to properly normalize them before matching against globs.
        //
        // FIXME: This code should be moved to the `traverse_inputs` function in `biome_cli/src/traverse.rs`.
        // Unfortunately moving this code to the `traverse_inputs` function makes many tests fail.
        // The issue is coming from some bugs in our test infra.
        // See https://github.com/biomejs/biome/pull/5017
        let path = match std::path::Path::new(&path).absolutize() {
            Ok(std::borrow::Cow::Owned(absolutized)) => Utf8PathBuf::from_path_buf(absolutized)
                .expect("Absolute path must be correctly parsed"),
            _ => path,
        };
        let file_type = match path.metadata() {
            Ok(meta) => meta.file_type(),
            Err(err) => {
                ctx.push_diagnostic(IoError::from(err).with_file_path(path.to_string()));
                return;
            }
        };
        handle_any_file(&self.scope, ctx, path, file_type, None);
    }

    fn handle(&self, context: &'scope dyn TraversalContext, path: Utf8PathBuf) {
        self.scope.spawn(move |_| {
            context.handle_path(BiomePath::new(path));
        });
    }
}

/// Traverse a single directory
fn handle_dir<'scope>(
    scope: &Scope<'scope>,
    ctx: &'scope dyn TraversalContext,
    path: &Utf8Path,
    // The unresolved origin path in case the directory is behind a symbolic link
    origin_path: Option<Utf8PathBuf>,
) {
    let iter = match path.read_dir_utf8() {
        Ok(iter) => iter,
        Err(err) => {
            ctx.push_diagnostic(IoError::from(err).with_file_path(path.to_string()));
            return;
        }
    };

    for entry in iter {
        match entry {
            Ok(entry) => handle_dir_entry(scope, ctx, entry, origin_path.clone()),
            Err(err) => {
                ctx.push_diagnostic(IoError::from(err).with_file_path(path.to_string()));
            }
        }
    }
}

/// Traverse a single directory entry, scheduling any file to execute the context
/// handler and sub-directories for subsequent traversal
fn handle_dir_entry<'scope>(
    scope: &Scope<'scope>,
    ctx: &'scope dyn TraversalContext,
    entry: Utf8DirEntry,
    // The unresolved origin path in case the directory is behind a symbolic link
    origin_path: Option<Utf8PathBuf>,
) {
    let path = entry.path();
    let file_type = match entry.file_type() {
        Ok(file_type) => file_type,
        Err(err) => {
            ctx.push_diagnostic(IoError::from(err).with_file_path(path.to_string()));
            return;
        }
    };
    handle_any_file(scope, ctx, path.to_path_buf(), file_type, origin_path);
}

fn handle_any_file<'scope>(
    scope: &Scope<'scope>,
    ctx: &'scope dyn TraversalContext,
    mut path: Utf8PathBuf,
    mut file_type: FileType,
    // The unresolved origin path in case the directory is behind a symbolic link
    mut origin_path: Option<Utf8PathBuf>,
) {
    if !ctx.interner().intern_path(path.clone()) {
        // If the path was already inserted, it could have been pointed at by
        // multiple symlinks. No need to traverse again.
        return;
    }

    if file_type.is_symlink() {
        if !ctx.can_handle(&BiomePath::new(path.clone())) {
            return;
        }

        let (target_path, target_file_type) = match expand_symbolic_link(&path) {
            Ok((target_path, target_file_type)) => (target_path, target_file_type),
            Err(error) => {
                ctx.push_diagnostic(error);
                return;
            }
        };

        if !ctx.interner().intern_path(target_path.clone()) {
            // If the path was already inserted, it could have been pointed at by
            // multiple symlinks. No need to traverse again.
            return;
        }

        if target_file_type.is_dir() {
            scope.spawn(move |scope| {
                handle_dir(scope, ctx, &target_path, Some(path));
            });
            return;
        }

        path = target_path;
        file_type = target_file_type;
    }

    // In case the file is inside a directory that is behind a symbolic link,
    // the unresolved origin path is used to construct a new path.
    // This is required to support ignore patterns to symbolic links.
    let biome_path = if let Some(old_origin_path) = &origin_path {
        if let Some(file_name) = path.file_name() {
            let new_origin_path = old_origin_path.join(file_name);
            origin_path = Some(new_origin_path.clone());
            BiomePath::new(new_origin_path)
        } else {
            ctx.push_diagnostic(Error::from(FileSystemDiagnostic {
                path: path.to_string(),
                error_kind: FsErrorKind::UnknownFileType,
                severity: Severity::Warning,
                source: None,
            }));
            return;
        }
    } else {
        BiomePath::new(&path)
    };

    // Performing this check here let's us skip unsupported
    // files entirely, as well as silently ignore unsupported files when
    // doing a directory traversal, but printing an error message if the
    // user explicitly requests an unsupported file to be handled.
    // This check also works for symbolic links.
    if !ctx.can_handle(&biome_path) {
        return;
    }

    if file_type.is_dir() {
        let path_buf = path.to_path_buf();
        scope.spawn(move |scope| {
            handle_dir(scope, ctx, &path_buf, origin_path);
        });
        if ctx.should_store_dirs() {
            ctx.store_path(BiomePath::new(path));
        }
        return;
    }

    if file_type.is_file() {
        ctx.store_path(BiomePath::new(path));
        return;
    }

    ctx.push_diagnostic(Error::from(FileSystemDiagnostic {
        path: path.to_string(),
        error_kind: FsErrorKind::from(file_type),
        severity: Severity::Warning,
        source: None,
    }));
}

fn path_kind_from_metadata(
    metadata: io::Result<Metadata>,
    path: &Utf8Path,
) -> Result<PathKind, FileSystemDiagnostic> {
    match metadata {
        Ok(metadata) => {
            let is_symlink = metadata.is_symlink();
            if metadata.is_dir() {
                Ok(PathKind::Directory { is_symlink })
            } else if metadata.is_file() || is_symlink {
                Ok(PathKind::File { is_symlink })
            } else {
                Err(FileSystemDiagnostic {
                    path: path.to_string(),
                    severity: Severity::Error,
                    error_kind: FsErrorKind::UnknownFileType,
                    source: None,
                })
            }
        }
        Err(error) => Err(FileSystemDiagnostic {
            path: path.to_string(),
            severity: Severity::Error,
            error_kind: FsErrorKind::CantReadFile,
            source: Some(Error::from(IoError::from(error))),
        }),
    }
}

impl From<FileType> for FsErrorKind {
    fn from(_: FileType) -> Self {
        Self::UnknownFileType
    }
}

/// Testing utility that creates a working directory inside the
/// temporary OS folder.
pub struct TemporaryFs {
    /// The current working directory. It's the OS temporary folder joined with a file
    /// name passed in the [TemporaryFs::new] function
    pub working_directory: Utf8PathBuf,
    files: Vec<(Utf8PathBuf, String)>,
    project_directory: Utf8PathBuf,
}

impl TemporaryFs {
    /// Creates a temporary directory named using `directory_name`
    pub fn new(directory_name: &str) -> Self {
        let path = temp_dir().join(directory_name);
        if path.exists() {
            fs::remove_dir_all(path.as_path()).unwrap();
        }
        fs::create_dir(&path).unwrap();

        // On macOS, the temporary directory is in `/var`, which is a symlink to `/private/var`.
        // We need to get the actual path here, or we will get path inconsistency.
        #[cfg(target_os = "macos")]
        let path = fs::canonicalize(path).unwrap();

        Self {
            working_directory: Utf8PathBuf::from_path_buf(path.clone()).unwrap(),
            files: Vec::new(),
            project_directory: Utf8PathBuf::from_path_buf(path).unwrap(),
        }
    }

    /// Creates a file under the working directory
    pub fn create_file(&mut self, name: &str, content: &str) -> Utf8PathBuf {
        let path = self.working_directory.join(name);
        fs::create_dir_all(path.parent().expect("parent dir exists."))
            .expect("Temporary directory to exist and being writable");
        fs::write(path.as_std_path(), content)
            .expect("Temporary directory to exist and being writable");
        self.files.push((path.clone(), content.to_string()));
        path
    }

    pub fn create_folder(&mut self, name: &str) {
        let path = self.working_directory.join(name);
        fs::create_dir_all(path.parent().expect("parent dir exists."))
            .expect("Temporary directory to exist and being writable");
    }

    pub fn append_to_working_directory(&mut self, path: &str) {
        self.working_directory = self.project_directory.join(path);
    }

    /// Returns the path to use when running the CLI
    pub fn cli_path(&self) -> &str {
        self.working_directory.as_str()
    }

    /// Returns an instance of [OsFileSystem] given the current working directory
    pub fn create_os(&self) -> OsFileSystem {
        OsFileSystem::new(self.working_directory.clone())
    }

    /// Returns an instance of [MemoryFileSystem]. The files saved in the file system
    /// will be stripped of the working directory path, making snapshots predictable.
    pub fn create_mem(&self) -> MemoryFileSystem {
        let fs = MemoryFileSystem::default();
        for (path, content) in self.files.iter() {
            fs.insert(
                path.clone()
                    .strip_prefix(self.project_directory.as_str())
                    .expect("Working directory")
                    .to_path_buf(),
                content.as_bytes(),
            );
        }

        fs
    }
}
