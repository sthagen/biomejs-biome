use biome_formatter::Printed;
use biome_html_formatter::{HtmlFormatOptions, format_node};
use biome_html_parser::parse_html;
use biome_html_syntax::{HtmlFileSource, HtmlRoot};
use biome_rowan::AstNode;
use biome_test_utils::BenchCase;
use criterion::{BenchmarkId, Criterion, Throughput, black_box, criterion_group, criterion_main};
use std::collections::HashMap;

#[cfg(target_os = "windows")]
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

#[cfg(all(
    any(target_os = "macos", target_os = "linux"),
    not(target_env = "musl"),
))]
#[global_allocator]
static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

// Jemallocator does not work on aarch64 with musl, so we'll use the system allocator instead
#[cfg(all(target_env = "musl", target_os = "linux", target_arch = "aarch64"))]
#[global_allocator]
static GLOBAL: std::alloc::System = std::alloc::System;
fn bench_formatter(criterion: &mut Criterion) {
    let mut all_suites = HashMap::new();
    all_suites.insert("html", include_str!("libs-html.txt"));
    let mut libs = vec![];
    libs.extend(all_suites.values().flat_map(|suite| suite.lines()));

    let mut group = criterion.benchmark_group("html_formatter");

    for lib in libs {
        let test_case = BenchCase::try_from(lib);

        match test_case {
            Ok(test_case) => {
                let code = test_case.code();
                let file_source = HtmlFileSource::try_from(test_case.path()).unwrap_or_default();
                let parsed = parse_html(code, file_source);
                group.throughput(Throughput::Bytes(code.len() as u64));
                group.bench_with_input(
                    BenchmarkId::from_parameter(test_case.filename()),
                    &code,
                    |b, _| {
                        fn format(root: HtmlRoot) -> Printed {
                            let formatted =
                                format_node(HtmlFormatOptions::default(), root.syntax()).unwrap();
                            let printed = formatted.print();
                            drop(formatted);
                            printed.expect("Document to be valid")
                        }
                        b.iter(|| {
                            black_box(format(parsed.tree()));
                        })
                    },
                );
            }
            Err(e) => println!("{e:?}"),
        }
    }
    group.finish();
}

criterion_group!(html_formatter, bench_formatter);
criterion_main!(html_formatter);
