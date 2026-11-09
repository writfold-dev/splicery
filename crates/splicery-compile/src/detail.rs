//! Bounded, typed facts that explain a selector refusal (spec §9). A detail
//! names at most `MAX_DETAIL_SITES` original-image spans and a count; it
//! carries no file bytes, so the caller decides what context it may show.
//! Near-miss searches run only on the refusal path, charge the same work
//! budget, and are dropped (never refused) when that budget is exhausted.

use splicery_domain::{RecipeSpan, SourceSpan};

use crate::environment::Lowering;
use crate::{CompileDiagnostic, CompileError};

pub const MAX_DETAIL_SITES: usize = 4;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DetailReason {
    /// The selector matched `count` sites in its window.
    Matches,
    /// Nothing matched in the window; `count` occurrences lie outside `within`.
    OutsideWithin,
    /// Nothing matched exactly; `count` lines match after trimming spaces and tabs.
    Whitespace,
    /// Nothing matched exactly; `count` occurrences match with LF and CRLF exchanged.
    LineEnding,
    /// The two anchors are reversed, coincident or overlapping.
    Anchors,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SelectorDetail {
    pub reason: DetailReason,
    pub count: usize,
    pub sites: [Option<SourceSpan>; MAX_DETAIL_SITES],
}

impl SelectorDetail {
    pub(crate) fn new(reason: DetailReason) -> Self {
        Self {
            reason,
            count: 0,
            sites: [None; MAX_DETAIL_SITES],
        }
    }

    pub(crate) fn record(&mut self, span: SourceSpan) {
        if let Some(slot) = self.sites.get_mut(self.count) {
            *slot = Some(span);
        }
        self.count = self.count.saturating_add(1);
    }

    pub fn recorded(&self) -> impl Iterator<Item = SourceSpan> + '_ {
        self.sites.iter().flatten().copied()
    }
}

pub(crate) fn refusal(
    kind: CompileError,
    origin: RecipeSpan,
    detail: Option<SelectorDetail>,
) -> CompileDiagnostic {
    CompileDiagnostic {
        kind,
        origin,
        detail: detail.filter(|detail| detail.count != 0).map(Box::new),
    }
}

/// Every occurrence of `needle` starting in `[lo, hi)` and ending by `hi`,
/// including overlapping starts, or `None` if the budget cannot pay for it.
pub(crate) fn occurrences(
    lowering: &mut Lowering,
    needle: &[u8],
    lo: usize,
    hi: usize,
    reason: DetailReason,
) -> Option<SelectorDetail> {
    let width = hi.checked_sub(lo)?;
    if needle.is_empty() || needle.len() > width {
        return None;
    }
    let positions = width - needle.len() + 1;
    lowering.budget.charge(positions).ok()?;
    lowering
        .budget
        .charge_repeated(needle.len(), positions)
        .ok()?;
    let mut detail = SelectorDetail::new(reason);
    for start in lo..lo + positions {
        if lowering.image.bytes().get(start..start + needle.len()) == Some(needle) {
            detail.record(SourceSpan::new(
                lowering.image,
                start,
                start + needle.len(),
            )?);
        }
    }
    Some(detail)
}

/// The needle with LF and CRLF exchanged, when it contains a line break.
pub(crate) fn other_line_ending(needle: &[u8]) -> Option<Vec<u8>> {
    if needle.windows(2).any(|pair| pair == b"\r\n") {
        let mut out = Vec::with_capacity(needle.len());
        let mut index = 0;
        while index < needle.len() {
            if needle.get(index..index + 2) == Some(b"\r\n") {
                index += 1;
            }
            out.push(needle[index]);
            index += 1;
        }
        return Some(out);
    }
    if !needle.contains(&b'\n') {
        return None;
    }
    let mut out = Vec::with_capacity(needle.len() * 2);
    for byte in needle {
        if *byte == b'\n' {
            out.push(b'\r');
        }
        out.push(*byte);
    }
    Some(out)
}

pub(crate) fn trim_blank(bytes: &[u8]) -> &[u8] {
    let start = bytes
        .iter()
        .position(|byte| !matches!(byte, b' ' | b'\t'))
        .unwrap_or(bytes.len());
    let end = bytes
        .iter()
        .rposition(|byte| !matches!(byte, b' ' | b'\t'))
        .map_or(start, |index| index + 1);
    &bytes[start..end]
}
