//! Actionable refusal text on stderr (spec §9). One fact per line so agents
//! can parse it; the first line keeps the typed kind name. Match sites are
//! reported as line numbers and byte ranges. Bytes of the file appear only
//! when the caller named that file on the command line with `--file`, and
//! then at most `MAX_CONTEXT` bytes per site for at most four sites.

use std::fmt::Write as _;

use splicery_compile::detail::{DetailReason, SelectorDetail};
use splicery_compile::{CompileDiagnostic, CompileError};
use splicery_kernel::KernelError;
use splicery_syntax::Diagnostic;

const MAX_CONTEXT: usize = 120;

/// The target file as the refusal may describe it.
pub(crate) struct Target<'a> {
    pub(crate) path: &'a str,
    pub(crate) bytes: &'a [u8],
    /// True only when the caller passed this file with `--file`.
    pub(crate) show_context: bool,
}

pub(crate) fn syntax(diagnostic: &Diagnostic) -> String {
    let origin = diagnostic.origin;
    format!(
        "refused: {:?} at recipe [{},{}): {}",
        diagnostic.kind, origin.start, origin.end, diagnostic.message
    )
}

pub(crate) fn kernel(error: KernelError, path: &str) -> String {
    let hint = match error {
        KernelError::Overlap => {
            "\n  hint: two edits overlap or insert at the same point; every operand reads \
             the original, so coalesce same-anchor insertions into one payload"
        }
        _ => "",
    };
    format!("refused: {error:?} in {path}{hint}")
}

fn line_number(bytes: &[u8], at: usize) -> usize {
    bytes[..at.min(bytes.len())]
        .iter()
        .filter(|byte| **byte == b'\n')
        .count()
        + 1
}

fn line_text(bytes: &[u8], at: usize) -> String {
    let at = at.min(bytes.len());
    let start = bytes[..at]
        .iter()
        .rposition(|byte| *byte == b'\n')
        .map_or(0, |index| index + 1);
    let end = bytes[start..]
        .iter()
        .position(|byte| *byte == b'\n' || *byte == b'\r')
        .map_or(bytes.len(), |offset| start + offset);
    let mut cut = end.min(start + MAX_CONTEXT);
    while cut > start && cut < end && bytes[cut] & 0xc0 == 0x80 {
        cut -= 1;
    }
    let mut text = match std::str::from_utf8(&bytes[start..cut]) {
        Ok(text) => text.chars().flat_map(char::escape_debug).collect(),
        Err(_) => bytes[start..cut].escape_ascii().to_string(),
    };
    if cut < end {
        text.push_str("...");
    }
    text
}

fn sites_heading(detail: &SelectorDetail) -> String {
    let count = detail.count;
    let mut heading = match detail.reason {
        DetailReason::Matches => format!("matched {count} sites"),
        DetailReason::OutsideWithin => format!("{count} match(es) outside `within`"),
        DetailReason::Whitespace => format!("{count} line(s) match after trimming spaces and tabs"),
        DetailReason::LineEnding => format!("{count} match(es) with LF and CRLF exchanged"),
        DetailReason::Anchors => "anchors".to_string(),
    };
    let shown = detail.recorded().count();
    if shown < count {
        let _ = write!(heading, " (first {shown})");
    }
    heading
}

fn hint(diagnostic: &CompileDiagnostic) -> Option<String> {
    let count = diagnostic.detail.as_ref().map_or(0, |detail| detail.count);
    let reason = diagnostic.detail.as_ref().map(|detail| detail.reason);
    let text = match (diagnostic.kind, reason) {
        (CompileError::AmbiguousSelector, _) => {
            "narrow it with `within` a window (`between` two unique lines, or `from @a to @b`), \
             or add distinguishing context to the needle"
                .to_string()
        }
        (CompileError::MissingSelector, Some(DetailReason::OutsideWithin)) => {
            format!("{count} occurrence(s) exist outside the `within` window; widen or drop it")
        }
        (CompileError::MissingSelector, Some(DetailReason::Whitespace)) => {
            "copy the exact indentation and trailing spaces, or use `one text` for part of a line"
                .to_string()
        }
        (CompileError::MissingSelector, Some(DetailReason::LineEnding)) => {
            "the file uses the other line ending; write \\r\\n or \\n to match it".to_string()
        }
        (CompileError::MissingSelector, Some(DetailReason::Anchors)) => {
            "the first anchor must end at or before the second starts; reversed, coincident \
             or overlapping anchors refuse"
                .to_string()
        }
        (CompileError::MissingSelector, _) => {
            "nothing matched; selectors compare exact bytes, so copy quotes, whitespace and \
             line endings from the file"
                .to_string()
        }
        (CompileError::InvalidCapture, _) => {
            "`lines like`, `between` and `whole` need a line binding (`one line`, or a \
             whole-line match)"
                .to_string()
        }
        (CompileError::UnknownBinding, _) => {
            "bind the handle earlier in the recipe; bindings are visible only after they \
             appear"
                .to_string()
        }
        (CompileError::DuplicateBinding, _) => "each handle is bound once".to_string(),
        _ => return None,
    };
    Some(text)
}

pub(crate) fn compile(diagnostic: &CompileDiagnostic, target: &Target<'_>) -> String {
    let origin = diagnostic.origin;
    let mut out = format!(
        "refused: {:?} at recipe [{},{}) in {}",
        diagnostic.kind, origin.start, origin.end, target.path
    );
    if let Some(detail) = &diagnostic.detail {
        let sites: Vec<String> = detail
            .recorded()
            .map(|span| {
                let line = line_number(target.bytes, span.lo());
                format!("line {line} bytes [{},{})", span.lo(), span.hi())
            })
            .collect();
        let _ = write!(out, "\n  {}: {}", sites_heading(detail), sites.join("; "));
        if target.show_context {
            for span in detail.recorded() {
                let line = line_number(target.bytes, span.lo());
                let _ = write!(
                    out,
                    "\n  line {line}: {}",
                    line_text(target.bytes, span.lo())
                );
            }
        }
    }
    if let Some(hint) = hint(diagnostic) {
        let _ = write!(out, "\n  hint: {hint}");
    }
    out
}
