use splicery_domain::{
    CaptureBinding, FilePlan, FrozenSet, OriginalImage, Plan, Profile, RecipeSpan, ResolvedCapture,
    SourceSpan,
};
use splicery_syntax::ast::{Recipe, Statement};

mod environment;
mod file_plan;
mod iteration;
mod payload;
pub mod regex;
mod selector;

use environment::{Binding, CAPTURE_ORIGIN, Lowering, find_binding, validate_capture};
use file_plan::lower_create;
use iteration::lower_iteration;
use payload::lower_change;
use selector::resolve_selector;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CompileError {
    DuplicateBinding,
    UnknownBinding,
    MissingSelector,
    AmbiguousSelector,
    OutOfBounds,
    TooManyMatches,
    TooManySplices,
    Unsupported,
    AbsentGroup,
    InvalidCapture,
    WrongProfile,
    UnexpectedMatchCount,
    PayloadTooLarge,
    BudgetExceeded,
    Regex,
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

pub fn lower(
    recipe: &Recipe,
    image: &OriginalImage,
    captures: &[CaptureBinding],
    frozen: &[FrozenSet],
) -> Result<Plan, CompileDiagnostic> {
    match recipe {
        Recipe::Edit {
            raw, statements, ..
        } => {
            if *raw && image.profile() != Profile::Raw {
                return Err(CompileDiagnostic::new(
                    CompileError::WrongProfile,
                    CAPTURE_ORIGIN,
                ));
            }
            for (index, set) in frozen.iter().enumerate() {
                if frozen[..index]
                    .iter()
                    .any(|earlier| earlier.name == set.name)
                {
                    return Err(CompileDiagnostic::new(
                        CompileError::DuplicateBinding,
                        CAPTURE_ORIGIN,
                    ));
                }
            }
            lower_edit(statements, image, captures, frozen)
        }
        Recipe::Create { path, raw, payload } => lower_create(path, *raw, payload, image),
        Recipe::Rename {
            source,
            destination,
        } => Ok(Plan::File(FilePlan::Rename {
            source: source.clone(),
            destination: destination.clone(),
        })),
        Recipe::Remove { path } => Ok(Plan::File(FilePlan::Remove { path: path.clone() })),
    }
}

fn lower_edit(
    statements: &[Statement],
    image: &OriginalImage,
    captures: &[CaptureBinding],
    frozen: &[FrozenSet],
) -> Result<Plan, CompileDiagnostic> {
    let mut lowering = Lowering::new(image);
    let mut bindings = Vec::new();
    for capture in captures {
        if find_binding(&bindings, &capture.name).is_some() {
            return Err(CompileDiagnostic::new(
                CompileError::DuplicateBinding,
                CAPTURE_ORIGIN,
            ));
        }
        let span = SourceSpan::new(image.bytes().len(), capture.span.lo(), capture.span.hi())
            .ok_or_else(|| CompileDiagnostic::new(CompileError::OutOfBounds, CAPTURE_ORIGIN))?;
        let resolved = ResolvedCapture::plain(span);
        validate_capture(&resolved, &mut lowering, CAPTURE_ORIGIN)?;
        bindings.push(Binding {
            name: capture.name.clone(),
            capture: resolved,
        });
    }

    let mut retained_literals = 0usize;
    for statement in statements {
        match statement {
            Statement::Binding(binding) => {
                if find_binding(&bindings, &binding.name).is_some() {
                    return Err(CompileDiagnostic::new(
                        CompileError::DuplicateBinding,
                        binding.origin,
                    ));
                }
                let selected =
                    resolve_selector(&binding.selector, &mut lowering, &bindings, binding.origin)?;
                let capture = selected.resolved();
                bindings.push(Binding {
                    name: binding.name.clone(),
                    capture,
                });
            }
            Statement::Change(change) => {
                lower_change(change, &mut lowering, &bindings, &mut retained_literals)?
            }
            Statement::Iteration(iteration) => lower_iteration(
                iteration,
                frozen,
                &mut lowering,
                &mut bindings,
                &mut retained_literals,
            )?,
        }
    }
    Ok(Plan::Edit(lowering.splices))
}

#[cfg(test)]
mod tests;
