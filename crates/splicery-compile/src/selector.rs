use splicery_domain::{LineEnding, MAX_MATCHES, Profile, RecipeSpan, SourceSpan};
use splicery_syntax::ast::{Reference, Selector};

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
        return Err(CompileDiagnostic::new(
            CompileError::MissingSelector,
            origin,
        ));
    }

    let positions = window.width() - needle.len() + 1;
    lowering.charge(positions, origin)?;
    lowering.charge_repeated(needle.len(), positions, origin)?;
    let bytes = lowering.image.bytes();
    let last = window.hi() - needle.len();
    let mut start = window.lo();
    let mut found = None;
    let mut match_count = 0usize;
    loop {
        let end = start + needle.len();
        if bytes.get(start..end) == Some(needle) {
            match_count += 1;
            if match_count > MAX_MATCHES {
                return Err(CompileDiagnostic::new(CompileError::TooManyMatches, origin));
            }
            found = Some(start);
        }
        if start == last {
            break;
        }
        start += 1;
    }

    match (match_count, found) {
        (0, _) => Err(CompileDiagnostic::new(
            CompileError::MissingSelector,
            origin,
        )),
        (1, Some(start)) => {
            let span = SourceSpan::new(lowering.image, start, start + needle.len())
                .ok_or_else(|| CompileDiagnostic::new(CompileError::OutOfBounds, origin))?;
            Ok(CaptureView::plain(checked_span(lowering, span, origin)?))
        }
        (1, None) => Err(CompileDiagnostic::new(
            CompileError::MissingSelector,
            origin,
        )),
        _ => Err(CompileDiagnostic::new(
            CompileError::AmbiguousSelector,
            origin,
        )),
    }
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
    let bytes = lowering.image.bytes();
    let mut found = None;
    let mut match_count = 0usize;
    let mut start = 0usize;
    while start < bytes.len() {
        let mut end = start;
        while end < bytes.len() && bytes[end] != b'\n' {
            end += 1;
        }
        let terminated = end < bytes.len();
        let body_end = if terminated && end > start && bytes[end - 1] == b'\r' {
            end - 1
        } else {
            end
        };
        if start >= window.lo()
            && body_end <= window.hi()
            && body_end - start == needle.len()
            && &bytes[start..body_end] == needle
        {
            match_count += 1;
            if match_count > MAX_MATCHES {
                return Err(CompileDiagnostic::new(CompileError::TooManyMatches, origin));
            }
            let line = if !terminated {
                LineEnding::Unterminated
            } else if body_end != end {
                LineEnding::CrLf
            } else {
                LineEnding::Lf
            };
            let span = SourceSpan::new(lowering.image, start, body_end)
                .ok_or_else(|| CompileDiagnostic::new(CompileError::OutOfBounds, origin))?;
            found = Some(CaptureView {
                span,
                line: Some(line),
                groups: &[],
            });
        }
        start = if terminated { end + 1 } else { bytes.len() };
    }

    match (match_count, found) {
        (0, _) => Err(CompileDiagnostic::new(
            CompileError::MissingSelector,
            origin,
        )),
        (1, Some(capture)) => Ok(capture),
        (1, None) => Err(CompileDiagnostic::new(
            CompileError::MissingSelector,
            origin,
        )),
        _ => Err(CompileDiagnostic::new(
            CompileError::AmbiguousSelector,
            origin,
        )),
    }
}

fn resolve_between<'a>(
    first: &Reference,
    second: &Reference,
    lowering: &mut Lowering,
    bindings: &'a [Binding],
    origin: RecipeSpan,
) -> Result<CaptureView<'a>, CompileDiagnostic> {
    let first = resolve_reference(first, lowering, bindings, origin)?;
    let second = resolve_reference(second, lowering, bindings, origin)?;
    if first.span == second.span {
        return Err(CompileDiagnostic::new(
            CompileError::MissingSelector,
            origin,
        ));
    }
    let first_line = first
        .line
        .ok_or_else(|| CompileDiagnostic::new(CompileError::InvalidCapture, origin))?;
    if second.line.is_none() {
        return Err(CompileDiagnostic::new(CompileError::InvalidCapture, origin));
    }
    let eol = first_line
        .bytes()
        .ok_or_else(|| CompileDiagnostic::new(CompileError::MissingSelector, origin))?;
    let full_end = first
        .span
        .hi()
        .checked_add(eol.len())
        .ok_or_else(|| CompileDiagnostic::new(CompileError::OutOfBounds, origin))?;
    if full_end > second.span.lo() {
        return Err(CompileDiagnostic::new(
            CompileError::MissingSelector,
            origin,
        ));
    }
    let span = SourceSpan::new(lowering.image, full_end, second.span.lo())
        .ok_or_else(|| CompileDiagnostic::new(CompileError::OutOfBounds, origin))?;
    Ok(CaptureView::plain(checked_span(lowering, span, origin)?))
}
