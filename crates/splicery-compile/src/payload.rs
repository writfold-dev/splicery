use splicery_domain::{
    EditTarget, LineEnding, MAX_PAYLOAD_BYTES, Part, Profile, RecipeSpan, Splice,
};
use splicery_syntax::ast::{Change, ChangeKind, Piece};

use crate::environment::{Binding, Lowering};
use crate::selector::{resolve_reference, resolve_selector};
use crate::{CompileDiagnostic, CompileError};

pub(crate) fn lower_change(
    change: &Change,
    lowering: &mut Lowering,
    bindings: &[Binding],
    retained_literals: &mut usize,
) -> Result<(), CompileDiagnostic> {
    if change.kind == ChangeKind::Delete && !change.payload.is_empty() {
        return Err(CompileDiagnostic::new(
            CompileError::Unsupported,
            change.origin,
        ));
    }
    let selected = resolve_selector(&change.selector, lowering, bindings, change.origin)?;
    let replacement = lower_payload(
        &change.payload,
        lowering,
        bindings,
        change.origin,
        retained_literals,
    )?;
    validate_text_payload(&replacement, lowering, change.origin)?;
    let image_len = lowering.image.bytes().len();
    let target = match change.kind {
        ChangeKind::Replace | ChangeKind::Delete => {
            EditTarget::new(image_len, selected.span.lo(), selected.span.hi())
        }
        ChangeKind::Before => EditTarget::insertion(image_len, selected.span.lo()),
        ChangeKind::After => EditTarget::insertion(image_len, selected.span.hi()),
    }
    .ok_or_else(|| CompileDiagnostic::new(CompileError::OutOfBounds, change.origin))?;
    lowering.push_splice(Splice {
        target,
        replacement,
        origin: change.origin,
    })
}

fn validate_text_payload(
    parts: &[Part],
    lowering: &mut Lowering,
    origin: RecipeSpan,
) -> Result<(), CompileDiagnostic> {
    if lowering.image.profile() != Profile::Text {
        return Ok(());
    }
    let split_encoding = parts
        .iter()
        .any(|part| matches!(part, Part::Literal(bytes) if std::str::from_utf8(bytes).is_err()));
    if split_encoding {
        let rendered = render_parts(parts, lowering, origin)?;
        if std::str::from_utf8(&rendered).is_err() {
            return Err(CompileDiagnostic::new(CompileError::WrongProfile, origin));
        }
    }
    Ok(())
}

fn lower_payload(
    pieces: &[Piece],
    lowering: &mut Lowering,
    bindings: &[Binding],
    origin: RecipeSpan,
    retained_literals: &mut usize,
) -> Result<Vec<Part>, CompileDiagnostic> {
    let mut parts = Vec::with_capacity(pieces.len());
    for piece in pieces {
        let part = match piece {
            Piece::Literal(bytes) => {
                lowering.charge(bytes.len(), origin)?;
                *retained_literals = retained_literals
                    .checked_add(bytes.len())
                    .filter(|length| *length <= MAX_PAYLOAD_BYTES)
                    .ok_or_else(|| CompileDiagnostic::new(CompileError::PayloadTooLarge, origin))?;
                Part::Literal(bytes.clone())
            }
            Piece::Keep(reference) => {
                let capture = resolve_reference(reference, lowering, bindings, origin)?;
                Part::Copy(capture.span)
            }
            Piece::LinesLike { reference, lines } => {
                if lowering.image.profile() == Profile::Raw {
                    return Err(CompileDiagnostic::new(CompileError::WrongProfile, origin));
                }
                let capture = resolve_reference(reference, lowering, bindings, origin)?;
                let eol = capture
                    .line
                    .and_then(LineEnding::bytes)
                    .ok_or_else(|| CompileDiagnostic::new(CompileError::InvalidCapture, origin))?;
                let mut length = 0usize;
                for line in lines {
                    if std::str::from_utf8(line).is_err() {
                        return Err(CompileDiagnostic::new(CompileError::WrongProfile, origin));
                    }
                    if line.iter().any(|byte| matches!(byte, b'\r' | b'\n')) {
                        return Err(CompileDiagnostic::new(CompileError::Unsupported, origin));
                    }
                    length = length
                        .checked_add(line.len())
                        .and_then(|length| length.checked_add(eol.len()))
                        .filter(|length| *length <= MAX_PAYLOAD_BYTES)
                        .ok_or_else(|| {
                            CompileDiagnostic::new(CompileError::PayloadTooLarge, origin)
                        })?;
                }
                lowering.charge(length, origin)?;
                *retained_literals = retained_literals
                    .checked_add(length)
                    .filter(|total| *total <= MAX_PAYLOAD_BYTES)
                    .ok_or_else(|| CompileDiagnostic::new(CompileError::PayloadTooLarge, origin))?;
                let mut bytes = Vec::with_capacity(length);
                for line in lines {
                    bytes.extend_from_slice(line);
                    bytes.extend_from_slice(eol);
                }
                Part::Literal(bytes)
            }
            Piece::Indent { prefix, payload } => {
                if lowering.image.profile() == Profile::Raw {
                    return Err(CompileDiagnostic::new(CompileError::WrongProfile, origin));
                }
                if prefix.iter().any(|byte| !matches!(byte, b' ' | b'\t')) {
                    return Err(CompileDiagnostic::new(CompileError::Unsupported, origin));
                }
                let mut nested_retained = 0usize;
                let nested =
                    lower_payload(payload, lowering, bindings, origin, &mut nested_retained)?;
                let rendered = render_parts(&nested, lowering, origin)?;
                let indented = indent_bytes(prefix, &rendered, lowering, origin)?;
                *retained_literals = retained_literals
                    .checked_add(indented.len())
                    .filter(|length| *length <= MAX_PAYLOAD_BYTES)
                    .ok_or_else(|| CompileDiagnostic::new(CompileError::PayloadTooLarge, origin))?;
                Part::Literal(indented)
            }
        };
        parts.push(part);
    }
    Ok(parts)
}

fn render_parts(
    parts: &[Part],
    lowering: &mut Lowering,
    origin: RecipeSpan,
) -> Result<Vec<u8>, CompileDiagnostic> {
    let mut length = 0usize;
    for part in parts {
        let width = match part {
            Part::Literal(bytes) => bytes.len(),
            Part::Copy(span) => span.width(),
        };
        length = length
            .checked_add(width)
            .filter(|length| *length <= MAX_PAYLOAD_BYTES)
            .ok_or_else(|| CompileDiagnostic::new(CompileError::PayloadTooLarge, origin))?;
    }
    lowering.charge(length, origin)?;
    let mut rendered = Vec::with_capacity(length);
    for part in parts {
        match part {
            Part::Literal(bytes) => rendered.extend_from_slice(bytes),
            Part::Copy(span) => {
                rendered.extend_from_slice(&lowering.image.bytes()[span.lo()..span.hi()]);
            }
        }
    }
    Ok(rendered)
}

fn indent_bytes(
    prefix: &[u8],
    bytes: &[u8],
    lowering: &mut Lowering,
    origin: RecipeSpan,
) -> Result<Vec<u8>, CompileDiagnostic> {
    let line_count = bytes.iter().filter(|byte| **byte == b'\n').count();
    let extra = prefix
        .len()
        .checked_mul(line_count.saturating_add(1))
        .and_then(|extra| extra.checked_add(bytes.len()))
        .filter(|length| *length <= MAX_PAYLOAD_BYTES)
        .ok_or_else(|| CompileDiagnostic::new(CompileError::PayloadTooLarge, origin))?;
    lowering.charge(extra, origin)?;
    let mut output = Vec::with_capacity(extra);
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
        if body_end != start {
            output.extend_from_slice(prefix);
        }
        output.extend_from_slice(&bytes[start..end]);
        if terminated {
            output.push(b'\n');
        }
        start = if terminated { end + 1 } else { bytes.len() };
    }
    Ok(output)
}
