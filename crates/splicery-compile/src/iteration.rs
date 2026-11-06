use splicery_domain::{FrozenSet, MAX_MATCHES, Profile, RecipeSpan, ResolvedCapture, SourceSpan};
use splicery_syntax::ast::{Collection, Iteration};

use crate::environment::{Binding, Lowering, checked_span, find_binding, validate_capture};
use crate::payload::lower_change;
use crate::selector::resolve_reference;
use crate::{CompileDiagnostic, CompileError, regex};

pub(crate) fn lower_iteration(
    iteration: &Iteration,
    frozen: &[FrozenSet],
    lowering: &mut Lowering,
    bindings: &mut Vec<Binding>,
    retained_literals: &mut usize,
) -> Result<(), CompileDiagnostic> {
    if find_binding(bindings, &iteration.binder).is_some() {
        return Err(CompileDiagnostic::new(
            CompileError::DuplicateBinding,
            iteration.origin,
        ));
    }
    if lowering.image.profile() == Profile::Raw {
        return Err(CompileDiagnostic::new(
            CompileError::WrongProfile,
            iteration.origin,
        ));
    }
    match &iteration.collection {
        Collection::LineMatches {
            pattern,
            within,
            expect,
            origin,
        } => {
            let scope = match within {
                Some(reference) => {
                    Some(resolve_reference(reference, lowering, bindings, *origin)?.resolved())
                }
                None => None,
            };
            let capture_hash = lowering.capture_sha256(*origin)?;
            let set = regex::match_lines(
                lowering.image,
                &iteration.binder,
                pattern,
                scope,
                *expect,
                capture_hash,
                &mut lowering.budget,
            )
            .map_err(|error| regex_diagnostic(error, *origin))?;
            validate_frozen(&set, &iteration.binder, Some(*expect), lowering, *origin)?;
            expand_iteration(iteration, &set, lowering, bindings, retained_literals)
        }
        Collection::Frozen(reference) => {
            if reference.group.is_some() {
                return Err(CompileDiagnostic::new(
                    CompileError::Unsupported,
                    iteration.origin,
                ));
            }
            let set = frozen
                .iter()
                .find(|set| set.name == reference.name)
                .ok_or_else(|| {
                    CompileDiagnostic::new(CompileError::UnknownBinding, iteration.origin)
                })?;
            validate_frozen(set, &reference.name, None, lowering, iteration.origin)?;
            expand_iteration(iteration, set, lowering, bindings, retained_literals)
        }
    }
}

fn regex_diagnostic(error: regex::RegexError, origin: RecipeSpan) -> CompileDiagnostic {
    let kind = match error {
        regex::RegexError::WrongProfile => CompileError::WrongProfile,
        regex::RegexError::UnexpectedMatchCount { .. } => CompileError::UnexpectedMatchCount,
        regex::RegexError::TooManyMatches => CompileError::TooManyMatches,
        regex::RegexError::BudgetExceeded => CompileError::BudgetExceeded,
        regex::RegexError::ZeroWidthMatch
        | regex::RegexError::IncompleteMatch
        | regex::RegexError::OutOfScopeMatch => CompileError::InvalidCapture,
        regex::RegexError::SourceTooLong
        | regex::RegexError::InvalidPattern
        | regex::RegexError::AutomatonTooLarge
        | regex::RegexError::UnsupportedConstruct
        | regex::RegexError::InvalidCaptureName
        | regex::RegexError::DuplicateCaptureName
        | regex::RegexError::InvalidScope
        | regex::RegexError::InvalidUtf8 => CompileError::Regex,
    };
    CompileDiagnostic::new(kind, origin)
}

fn validate_frozen(
    set: &FrozenSet,
    name: &str,
    expected: Option<usize>,
    lowering: &mut Lowering,
    origin: RecipeSpan,
) -> Result<(), CompileDiagnostic> {
    if set.name != name
        || set.matcher_profile != regex::PROFILE_ID
        || set.capture_sha256 != lowering.capture_sha256(origin)?
    {
        return Err(CompileDiagnostic::new(CompileError::InvalidCapture, origin));
    }
    let scope = match set.scope {
        Some(span) => Some(checked_span(lowering, span, origin)?),
        None => None,
    };
    if set.members.len() > MAX_MATCHES {
        return Err(CompileDiagnostic::new(CompileError::TooManyMatches, origin));
    }
    if expected.is_some_and(|expected| expected != set.members.len()) {
        return Err(CompileDiagnostic::new(
            CompileError::UnexpectedMatchCount,
            origin,
        ));
    }
    let mut previous_end = 0usize;
    for (index, member) in set.members.iter().enumerate() {
        validate_capture(member, lowering, origin)?;
        if member.span.lo() == member.span.hi()
            || scope
                .is_some_and(|scope| member.span.lo() < scope.lo() || member.span.hi() > scope.hi())
            || (index != 0 && previous_end > member.span.lo())
        {
            return Err(CompileDiagnostic::new(CompileError::InvalidCapture, origin));
        }
        previous_end = member.span.hi();
    }
    Ok(())
}

fn expand_iteration(
    iteration: &Iteration,
    set: &FrozenSet,
    lowering: &mut Lowering,
    bindings: &mut Vec<Binding>,
    retained_literals: &mut usize,
) -> Result<(), CompileDiagnostic> {
    bindings.push(Binding {
        name: iteration.binder.clone(),
        capture: ResolvedCapture::plain(
            SourceSpan::insertion(lowering.image.bytes().len(), 0).ok_or_else(|| {
                CompileDiagnostic::new(CompileError::OutOfBounds, iteration.origin)
            })?,
        ),
    });
    let binder_index = bindings.len() - 1;
    for member in &set.members {
        lowering.charge(member.span.width(), iteration.origin)?;
        bindings[binder_index].capture = member.clone();
        for change in &iteration.changes {
            lower_change(change, lowering, bindings, retained_literals)?;
        }
    }
    bindings.pop();
    Ok(())
}
