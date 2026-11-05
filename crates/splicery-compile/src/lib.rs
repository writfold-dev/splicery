use splicery_domain::{
    CaptureBinding, MAX_MATCHES, OriginalImage, Part, RecipeSpan, SourceSpan, Splice,
};
use splicery_syntax::ast::{Change, ChangeKind, Piece, Recipe, Selector, Statement};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CompileError {
    DuplicateBinding,
    UnknownBinding,
    MissingSelector,
    AmbiguousSelector,
    OutOfBounds,
    TooManyMatches,
    Unsupported,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CompileDiagnostic {
    pub kind: CompileError,
    pub origin: RecipeSpan,
}

impl CompileDiagnostic {
    fn new(kind: CompileError, origin: RecipeSpan) -> Self {
        Self { kind, origin }
    }
}

const CAPTURE_ORIGIN: RecipeSpan = RecipeSpan { start: 0, end: 0 };

pub fn lower(
    recipe: &Recipe,
    image: &OriginalImage,
    captures: &[CaptureBinding],
) -> Result<Vec<Splice>, CompileDiagnostic> {
    let mut bindings = Vec::with_capacity(captures.len());
    for capture in captures {
        if find_binding(&bindings, &capture.name).is_some() {
            return Err(CompileDiagnostic::new(
                CompileError::DuplicateBinding,
                CAPTURE_ORIGIN,
            ));
        }
        let span = checked_span(image, capture.span, CAPTURE_ORIGIN)?;
        bindings.push((&capture.name, span));
    }

    let mut splices = Vec::new();
    for statement in &recipe.statements {
        match statement {
            Statement::Binding(binding) => {
                if find_binding(&bindings, &binding.name).is_some() {
                    return Err(CompileDiagnostic::new(
                        CompileError::DuplicateBinding,
                        binding.origin,
                    ));
                }
                let span = resolve_selector(&binding.selector, image, &bindings, binding.origin)?;
                bindings.push((&binding.name, span));
            }
            Statement::Change(change) => {
                let selected = resolve_selector(&change.selector, image, &bindings, change.origin)?;
                splices.push(lower_change(change, selected, image, &bindings)?);
            }
        }
    }
    Ok(splices)
}

fn find_binding(bindings: &[(&str, SourceSpan)], name: &str) -> Option<SourceSpan> {
    bindings
        .iter()
        .find(|(bound_name, _)| *bound_name == name)
        .map(|(_, span)| *span)
}

fn resolve_reference(
    name: &str,
    image: &OriginalImage,
    bindings: &[(&str, SourceSpan)],
    origin: RecipeSpan,
) -> Result<SourceSpan, CompileDiagnostic> {
    let span = find_binding(bindings, name)
        .ok_or_else(|| CompileDiagnostic::new(CompileError::UnknownBinding, origin))?;
    checked_span(image, span, origin)
}

fn checked_span(
    image: &OriginalImage,
    span: SourceSpan,
    origin: RecipeSpan,
) -> Result<SourceSpan, CompileDiagnostic> {
    SourceSpan::new(image.bytes().len(), span.lo(), span.hi())
        .ok_or_else(|| CompileDiagnostic::new(CompileError::OutOfBounds, origin))
}

fn resolve_selector(
    selector: &Selector,
    image: &OriginalImage,
    bindings: &[(&str, SourceSpan)],
    origin: RecipeSpan,
) -> Result<SourceSpan, CompileDiagnostic> {
    match selector {
        Selector::Reference(name) => resolve_reference(name, image, bindings, origin),
        Selector::OneText { needle, within } | Selector::OneHex { needle, within } => {
            resolve_one(needle, within.as_deref(), image, bindings, origin)
        }
        Selector::Range { lo, hi } => SourceSpan::new(image.bytes().len(), *lo, *hi)
            .ok_or_else(|| CompileDiagnostic::new(CompileError::OutOfBounds, origin)),
    }
}

fn resolve_one(
    needle: &[u8],
    within: Option<&str>,
    image: &OriginalImage,
    bindings: &[(&str, SourceSpan)],
    origin: RecipeSpan,
) -> Result<SourceSpan, CompileDiagnostic> {
    if needle.is_empty() {
        return Err(CompileDiagnostic::new(
            CompileError::MissingSelector,
            origin,
        ));
    }
    let window = match within {
        Some(name) => resolve_reference(name, image, bindings, origin)?,
        None => SourceSpan::new(image.bytes().len(), 0, image.bytes().len())
            .ok_or_else(|| CompileDiagnostic::new(CompileError::OutOfBounds, origin))?,
    };
    if needle.len() > window.width() {
        return Err(CompileDiagnostic::new(
            CompileError::MissingSelector,
            origin,
        ));
    }

    let bytes = image.bytes();
    let last = window.hi() - needle.len();
    let mut start = window.lo();
    let mut found = None;
    let mut match_count = 0;
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
        (1, Some(start)) => SourceSpan::new(image.bytes().len(), start, start + needle.len())
            .ok_or_else(|| CompileDiagnostic::new(CompileError::OutOfBounds, origin)),
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

fn lower_change(
    change: &Change,
    selected: SourceSpan,
    image: &OriginalImage,
    bindings: &[(&str, SourceSpan)],
) -> Result<Splice, CompileDiagnostic> {
    if change.kind == ChangeKind::Delete && !change.payload.is_empty() {
        return Err(CompileDiagnostic::new(
            CompileError::Unsupported,
            change.origin,
        ));
    }
    let replacement = lower_payload(&change.payload, image, bindings, change.origin)?;
    let target = match change.kind {
        ChangeKind::Replace | ChangeKind::Delete => Some(selected),
        ChangeKind::Before => SourceSpan::insertion(image.bytes().len(), selected.lo()),
        ChangeKind::After => SourceSpan::insertion(image.bytes().len(), selected.hi()),
    }
    .ok_or_else(|| CompileDiagnostic::new(CompileError::OutOfBounds, change.origin))?;

    Ok(Splice {
        target,
        replacement,
        origin: change.origin,
    })
}

fn lower_payload(
    pieces: &[Piece],
    image: &OriginalImage,
    bindings: &[(&str, SourceSpan)],
    origin: RecipeSpan,
) -> Result<Vec<Part>, CompileDiagnostic> {
    let mut parts = Vec::with_capacity(pieces.len());
    for piece in pieces {
        let part = match piece {
            Piece::Literal(bytes) => Part::Literal(bytes.clone()),
            Piece::Keep(name) => Part::Copy(resolve_reference(name, image, bindings, origin)?),
        };
        parts.push(part);
    }
    Ok(parts)
}

#[cfg(test)]
mod tests;
