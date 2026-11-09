//! Selectors defined by other captures: `between`, `from … to` and `whole`
//! (spec/language.md §3–§4). Each lowers to one plain `[lo,hi)` span in the
//! same original image; none of them changes the kernel or the IR.

use splicery_domain::{RecipeSpan, SourceSpan};
use splicery_syntax::ast::Reference;

use crate::detail::{DetailReason, SelectorDetail, refusal};
use crate::environment::{Binding, CaptureView, Lowering, checked_span};
use crate::selector::resolve_reference;
use crate::{CompileDiagnostic, CompileError};

fn plain<'a>(
    lowering: &Lowering,
    lo: usize,
    hi: usize,
    origin: RecipeSpan,
) -> Result<CaptureView<'a>, CompileDiagnostic> {
    let span = SourceSpan::new(lowering.image, lo, hi)
        .ok_or_else(|| CompileDiagnostic::new(CompileError::OutOfBounds, origin))?;
    Ok(CaptureView::plain(checked_span(lowering, span, origin)?))
}

fn misordered(first: SourceSpan, second: SourceSpan, origin: RecipeSpan) -> CompileDiagnostic {
    let mut detail = SelectorDetail::new(DetailReason::Anchors);
    detail.record(first);
    detail.record(second);
    refusal(CompileError::MissingSelector, origin, Some(detail))
}

/// `between @a and @b` = `[fullEnd(a), start(b))` for distinct whole-line
/// markers with a terminated `@a`.
pub(crate) fn resolve_between<'a>(
    first: &Reference,
    second: &Reference,
    lowering: &mut Lowering,
    bindings: &'a [Binding],
    origin: RecipeSpan,
) -> Result<CaptureView<'a>, CompileDiagnostic> {
    let first = resolve_reference(first, lowering, bindings, origin)?;
    let second = resolve_reference(second, lowering, bindings, origin)?;
    if first.span == second.span {
        return Err(misordered(first.span, second.span, origin));
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
        return Err(misordered(first.span, second.span, origin));
    }
    plain(lowering, full_end, second.span.lo(), origin)
}

/// `from @a to @b` = `[start(a), end(b))` for distinct anchors with
/// `end(a) <= start(b)`; reversed, coincident or overlapping anchors refuse.
pub(crate) fn resolve_from_to<'a>(
    first: &Reference,
    second: &Reference,
    lowering: &mut Lowering,
    bindings: &'a [Binding],
    origin: RecipeSpan,
) -> Result<CaptureView<'a>, CompileDiagnostic> {
    let first = resolve_reference(first, lowering, bindings, origin)?.span;
    let second = resolve_reference(second, lowering, bindings, origin)?.span;
    if first == second || first.hi() > second.lo() {
        return Err(misordered(first, second, origin));
    }
    plain(lowering, first.lo(), second.hi(), origin)
}

/// `whole @line` = `[start, fullEnd)`; an unterminated final line selects
/// its body through the end of the file. Other captures refuse.
pub(crate) fn resolve_whole<'a>(
    reference: &Reference,
    lowering: &mut Lowering,
    bindings: &'a [Binding],
    origin: RecipeSpan,
) -> Result<CaptureView<'a>, CompileDiagnostic> {
    let line = resolve_reference(reference, lowering, bindings, origin)?;
    let ending = line
        .line
        .ok_or_else(|| CompileDiagnostic::new(CompileError::InvalidCapture, origin))?;
    let full_end = line
        .span
        .hi()
        .checked_add(ending.bytes().map_or(0, <[u8]>::len))
        .ok_or_else(|| CompileDiagnostic::new(CompileError::OutOfBounds, origin))?;
    plain(lowering, line.span.lo(), full_end, origin)
}
