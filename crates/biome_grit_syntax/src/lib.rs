//! A crate for generated Syntax node definitions and utility macros.
//! Both rome_grit_lexer and biome_grit_parser rely on these definitions, therefore
//! they are wrapped in this crate to prevent cyclic dependencies

#![deny(clippy::use_self)]

#[macro_use]
mod generated;
pub mod file_source;
mod syntax_ext;
mod syntax_node;

pub use biome_rowan::{TextLen, TextRange, TextSize, TokenAtOffset, TriviaPieceKind, WalkEvent};
pub use generated::*;
pub use syntax_ext::*;
pub use syntax_node::*;

use GritSyntaxKind::*;
use biome_rowan::{AstNode, RawSyntaxKind, SyntaxKind};

impl From<u16> for GritSyntaxKind {
    fn from(d: u16) -> Self {
        assert!(d <= (Self::__LAST as u16));
        unsafe { std::mem::transmute::<u16, Self>(d) }
    }
}

impl From<GritSyntaxKind> for u16 {
    fn from(k: GritSyntaxKind) -> Self {
        k as Self
    }
}

impl GritSyntaxKind {
    /// Returns `true` for any contextual (await) or non-contextual keyword
    #[inline]
    pub const fn is_keyword(self) -> bool {
        (self as u16) <= (Self::RETURN_KW as u16) && (self as u16) >= (Self::SEQUENTIAL_KW as u16)
    }
}

impl biome_rowan::SyntaxKind for GritSyntaxKind {
    const TOMBSTONE: Self = TOMBSTONE;
    const EOF: Self = EOF;

    fn is_bogus(&self) -> bool {
        matches!(
            self,
            GRIT_BOGUS
                | GRIT_BOGUS_CONTAINER
                | GRIT_BOGUS_DEFINITION
                | GRIT_BOGUS_MAP_ELEMENT
                | GRIT_BOGUS_LANGUAGE_DECLARATION
                | GRIT_BOGUS_LANGUAGE_FLAVOR_KIND
                | GRIT_BOGUS_LITERAL
                | GRIT_BOGUS_NAMED_ARG
                | GRIT_BOGUS_PATTERN
                | GRIT_BOGUS_PREDICATE
                | GRIT_BOGUS_VERSION
        )
    }

    fn to_bogus(&self) -> Self {
        match self {
            kind if AnyGritLiteral::can_cast(*kind) => GRIT_BOGUS_LITERAL,
            kind if AnyGritPattern::can_cast(*kind) => GRIT_BOGUS_PATTERN,
            kind if AnyGritPredicate::can_cast(*kind) => GRIT_BOGUS_PREDICATE,
            kind if AnyGritContainer::can_cast(*kind) => GRIT_BOGUS_CONTAINER,
            kind if AnyGritLanguageDeclaration::can_cast(*kind) => GRIT_BOGUS_LANGUAGE_DECLARATION,
            kind if AnyGritLanguageFlavorKind::can_cast(*kind) => GRIT_BOGUS_LANGUAGE_FLAVOR_KIND,
            kind if AnyGritMapElement::can_cast(*kind) => GRIT_BOGUS_MAP_ELEMENT,
            kind if AnyGritMaybeNamedArg::can_cast(*kind) => GRIT_BOGUS_NAMED_ARG,
            kind if AnyGritDefinition::can_cast(*kind) => GRIT_BOGUS_DEFINITION,
            kind if AnyGritVersion::can_cast(*kind) => GRIT_BOGUS_VERSION,

            _ => GRIT_BOGUS,
        }
    }

    #[inline]
    fn to_raw(&self) -> RawSyntaxKind {
        RawSyntaxKind(*self as u16)
    }

    #[inline]
    fn from_raw(raw: RawSyntaxKind) -> Self {
        Self::from(raw.0)
    }

    fn is_root(&self) -> bool {
        GritRoot::can_cast(*self)
    }

    fn is_list(&self) -> bool {
        Self::is_list(*self)
    }

    fn is_trivia(self) -> bool {
        matches!(self, Self::NEWLINE | Self::WHITESPACE | Self::COMMENT)
    }

    fn to_string(&self) -> Option<&'static str> {
        Self::to_string(self)
    }
}

impl TryFrom<GritSyntaxKind> for TriviaPieceKind {
    type Error = ();

    fn try_from(value: GritSyntaxKind) -> Result<Self, Self::Error> {
        if value.is_trivia() {
            match value {
                GritSyntaxKind::NEWLINE => Ok(Self::Newline),
                GritSyntaxKind::WHITESPACE => Ok(Self::Whitespace),
                GritSyntaxKind::COMMENT => Ok(Self::SingleLineComment),
                _ => unreachable!("Not Trivia"),
            }
        } else {
            Err(())
        }
    }
}
