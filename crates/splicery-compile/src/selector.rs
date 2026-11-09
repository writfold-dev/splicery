use splicery_domain::{LineEnding, MAX_MATCHES, Profile, RecipeSpan, SourceSpan};
use splicery_syntax::ast::{Reference, Selector};

use crate::anchors::{resolve_between, resolve_from_to, resolve_whole};
use crate::detail::{
    DetailReason, SelectorDetail, occurrences, other_line_ending, refusal, trim_blank,
};
use crate::environment::{
    Binding, CaptureView, Lowering, checked_span, find_binding, validate_capture,
};
use crate::{CompileDiagnostic, CompileError};

pub(crate) fn resolve_reference<'a>(
    reference: &Reference,
    lowering: &mut Lowering,
    bindings: &'a [Binding],
    origin: RecipeSpan,
) -> Result<CaptureView<'a>, CompileDiagnostic> {
    let capture = find_binding(bindings, &reference.name)
        .ok_or_else(|| CompileDiagnostic::new(CompileError::UnknownBinding, origin))?;
    validate_capture(capture, lowering, origin)?;
    match &reference.group {
        None => Ok(CaptureView {
            span: capture.span,
            line: capture.line,
            groups: &capture.groups,
        }),
        Some(name) => {
            let group = capture
                .groups
                .iter()
                .find(|group| group.name == *name)
                .ok_or_else(|| CompileDiagnostic::new(CompileError::AbsentGroup, origin))?;
            let span = group
                .span
                .ok_or_else(|| CompileDiagnostic::new(CompileError::AbsentGroup, origin))?;
            checked_span(lowering, span, origin)?;
            Ok(CaptureView::plain(span))
        }
    }
}

pub(crate) fn resolve_selector<'a>(
    selector: &Selector,
    lowering: &mut Lowering,
    bindings: &'a [Binding],
    origin: RecipeSpan,
) -> Result<CaptureView<'a>, CompileDiagnostic> {
    match selector {
        Selector::Reference(reference) => resolve_reference(reference, lowering, bindings, origin),
        Selector::OneText { needle, within } => {
            if lowering.image.profile() == Profile::Raw {
                return Err(CompileDiagnostic::new(CompileError::WrongProfile, origin));
            }
            resolve_one(needle, within, lowering, bindings, origin)
        }
        Selector::OneHex { needle, within } => {
            resolve_one(needle, within, lowering, bindings, origin)
        }
        Selector::OneLine { needle, within } => {
            if lowering.image.profile() == Profile::Raw {
                return Err(CompileDiagnostic::new(CompileError::WrongProfile, origin));
            }
            resolve_one_line(needle, within, lowering, bindings, origin)
        }
        Selector::Between { first, second } => {
            resolve_between(first, second, lowering, bindings, origin)
        }
        Selector::FromTo { first, second } => {
            resolve_from_to(first, second, lowering, bindings, origin)
        }
        Selector::Whole(reference) => resolve_whole(reference, lowering, bindings, origin),
        Selector::Range { lo, hi } => {
            let span = SourceSpan::new(lowering.image, *lo, *hi)
                .ok_or_else(|| CompileDiagnostic::new(CompileError::OutOfBounds, origin))?;
            Ok(CaptureView::plain(checked_span(lowering, span, origin)?))
        }
    }
}

fn selection_window(
    within: &Option<Reference>,
    lowering: &mut Lowering,
    bindings: &[Binding],
    origin: RecipeSpan,
) -> Result<SourceSpan, CompileDiagnostic> {
    match within {
        Some(reference) => Ok(resolve_reference(reference, lowering, bindings, origin)?.span),
        None => SourceSpan::new(lowering.image, 0, lowering.image.bytes().len())
            .ok_or_else(|| CompileDiagnostic::new(CompileError::OutOfBounds, origin)),
    }
}

fn resolve_one<'a>(
    needle: &[u8],
    within: &Option<Reference>,
    lowering: &mut Lowering,
    bindings: &'a [Binding],
    origin: RecipeSpan,
) -> Result<CaptureView<'a>, CompileDiagnostic> {
    if needle.is_empty() {
        return Err(CompileDiagnostic::new(
            CompileError::MissingSelector,
            origin,
        ));
    }
    let window = selection_window(within, lowering, bindings, origin)?;
    if needle.len() > window.width() {
        return Err(missing_bytes(
            lowering,
            needle,
            window,
            within.is_some(),
            origin,
        ));
    }

    let positions = window.width() - needle.len() + 1;
    lowering.charge(positions, origin)?;
    lowering.charge_repeated(needle.len(), positions, origin)?;
    let mut matches = SelectorDetail::new(DetailReason::Matches);
    for start in window.lo()..window.lo() + positions {
        let end = start + needle.len();
        if lowering.image.bytes().get(start..end) == Some(needle) {
            if matches.count == MAX_MATCHES {
                return Err(CompileDiagnostic::new(CompileError::TooManyMatches, origin));
            }
            let span = SourceSpan::new(lowering.image, start, end)
                .ok_or_else(|| CompileDiagnostic::new(CompileError::OutOfBounds, origin))?;
            matches.record(span);
        }
    }
    match (matches.count, matches.sites[0]) {
        (0, _) | (1, None) => Err(missing_bytes(
            lowering,
            needle,
            window,
            within.is_some(),
            origin,
        )),
        (1, Some(span)) => Ok(CaptureView::plain(checked_span(lowering, span, origin)?)),
        _ => Err(refusal(
            CompileError::AmbiguousSelector,
            origin,
            Some(matches),
        )),
    }
}

/// A missing `one text`/`one hex`: report occurrences outside `within`, or
/// occurrences with the other line ending, when the budget allows.
fn missing_bytes(
    lowering: &mut Lowering,
    needle: &[u8],
    window: SourceSpan,
    scoped: bool,
    origin: RecipeSpan,
) -> CompileDiagnostic {
    let image_len = lowering.image.bytes().len();
    let mut detail = None;
    if scoped {
        detail = occurrences(lowering, needle, 0, image_len, DetailReason::OutsideWithin)
            .filter(|found| found.count != 0);
    }
    if detail.is_none()
        && let Some(variant) = other_line_ending(needle)
    {
        detail = occurrences(
            lowering,
            &variant,
            window.lo(),
            window.hi(),
            DetailReason::LineEnding,
        );
    }
    refusal(CompileError::MissingSelector, origin, detail)
}

struct LocatedLine {
    start: usize,
    body_end: usize,
    ending: LineEnding,
}

fn logical_lines(bytes: &[u8]) -> impl Iterator<Item = LocatedLine> + '_ {
    let mut start = 0usize;
    std::iter::from_fn(move || {
        if start >= bytes.len() {
            return None;
        }
        let end = bytes[start..]
            .iter()
            .position(|byte| *byte == b'\n')
            .map_or(bytes.len(), |offset| start + offset);
        let terminated = end < bytes.len();
        let crlf = terminated && end > start && bytes[end - 1] == b'\r';
        let line = LocatedLine {
            start,
            body_end: if crlf { end - 1 } else { end },
            ending: match (terminated, crlf) {
                (false, _) => LineEnding::Unterminated,
                (true, true) => LineEnding::CrLf,
                (true, false) => LineEnding::Lf,
            },
        };
        start = if terminated { end + 1 } else { bytes.len() };
        Some(line)
    })
}

fn resolve_one_line<'a>(
    needle: &[u8],
    within: &Option<Reference>,
    lowering: &mut Lowering,
    bindings: &'a [Binding],
    origin: RecipeSpan,
) -> Result<CaptureView<'a>, CompileDiagnostic> {
    let window = selection_window(within, lowering, bindings, origin)?;
    let image_len = lowering.image.bytes().len();
    lowering.charge(image_len, origin)?;
    let mut matches = SelectorDetail::new(DetailReason::Matches);
    let mut found = None;
    for line in logical_lines(lowering.image.bytes()) {
        if line.start >= window.lo()
            && line.body_end <= window.hi()
            && &lowering.image.bytes()[line.start..line.body_end] == needle
        {
            if matches.count == MAX_MATCHES {
                return Err(CompileDiagnostic::new(CompileError::TooManyMatches, origin));
            }
            let span = SourceSpan::new(lowering.image, line.start, line.body_end)
                .ok_or_else(|| CompileDiagnostic::new(CompileError::OutOfBounds, origin))?;
            matches.record(span);
            found = Some(CaptureView {
                span,
                line: Some(line.ending),
                groups: &[],
            });
        }
    }
    match (matches.count, found) {
        (1, Some(capture)) => Ok(capture),
        (0, _) | (1, None) => Err(missing_line(
            lowering,
            needle,
            window,
            within.is_some(),
            origin,
        )),
        _ => Err(refusal(
            CompileError::AmbiguousSelector,
            origin,
            Some(matches),
        )),
    }
}

/// A missing `one line`: report lines that match after trimming spaces and
/// tabs, or exact lines outside `within`, when the budget allows.
fn missing_line(
    lowering: &mut Lowering,
    needle: &[u8],
    window: SourceSpan,
    scoped: bool,
    origin: RecipeSpan,
) -> CompileDiagnostic {
    let image_len = lowering.image.bytes().len();
    if lowering.budget.charge(image_len).is_err() {
        return CompileDiagnostic::new(CompileError::MissingSelector, origin);
    }
    let wanted = trim_blank(needle);
    let mut trimmed = SelectorDetail::new(DetailReason::Whitespace);
    let mut outside = SelectorDetail::new(DetailReason::OutsideWithin);
    for line in logical_lines(lowering.image.bytes()) {
        let body = &lowering.image.bytes()[line.start..line.body_end];
        let Some(span) = SourceSpan::new(lowering.image, line.start, line.body_end) else {
            continue;
        };
        let inside = line.start >= window.lo() && line.body_end <= window.hi();
        if inside && !wanted.is_empty() && trim_blank(body) == wanted {
            trimmed.record(span);
        } else if scoped && !inside && body == needle {
            outside.record(span);
        }
    }
    let detail = if trimmed.count != 0 { trimmed } else { outside };
    refusal(CompileError::MissingSelector, origin, Some(detail))
}
