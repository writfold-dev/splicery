pub mod ast;
mod lexer;
mod parser;

use splicery_domain::RecipeSpan;

pub use parser::parse_recipe;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DiagnosticKind {
    Encoding,
    Syntax,
    Limit,
    Unsupported,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Diagnostic {
    pub kind: DiagnosticKind,
    pub origin: RecipeSpan,
    pub message: String,
}

pub(crate) type Parsed<T> = Result<T, Diagnostic>;

pub(crate) fn diagnostic(
    kind: DiagnosticKind,
    start: usize,
    end: usize,
    message: &str,
) -> Diagnostic {
    Diagnostic {
        kind,
        origin: RecipeSpan { start, end },
        message: message.to_owned(),
    }
}

pub(crate) struct Parser<'a> {
    pub(crate) bytes: &'a [u8],
    pub(crate) text: &'a str,
    pub(crate) at: usize,
}

#[cfg(test)]
mod anchor_tests;
#[cfg(test)]
mod tests;
