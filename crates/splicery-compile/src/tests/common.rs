use super::*;

pub(super) const ORIGIN: RecipeSpan = RecipeSpan { start: 2, end: 7 };

pub(super) fn recipe(statements: Vec<Statement>) -> Recipe {
    Recipe::Edit {
        path: "file".into(),
        raw: false,
        statements,
    }
}

pub(super) fn raw_recipe(statements: Vec<Statement>) -> Recipe {
    Recipe::Edit {
        path: "file".into(),
        raw: true,
        statements,
    }
}

pub(super) fn image_with(bytes: &[u8], profile: Profile) -> Result<OriginalImage, ()> {
    OriginalImage::new(bytes.to_vec(), profile).ok_or(())
}

pub(super) fn image(bytes: &[u8]) -> Result<OriginalImage, ()> {
    image_with(bytes, Profile::Raw)
}

pub(super) fn text_image(bytes: &[u8]) -> Result<OriginalImage, ()> {
    image_with(bytes, Profile::Text)
}

pub(super) fn reference(name: &str) -> Reference {
    Reference {
        name: name.into(),
        group: None,
    }
}

pub(super) fn group(name: &str, group: &str) -> Reference {
    Reference {
        name: name.into(),
        group: Some(group.into()),
    }
}

pub(super) fn range(lo: usize, hi: usize) -> Selector {
    Selector::Range { lo, hi }
}

pub(super) fn change(kind: ChangeKind, selector: Selector, payload: Vec<Piece>) -> Statement {
    Statement::Change(Change {
        kind,
        selector,
        payload,
        origin: ORIGIN,
    })
}

pub(super) fn binding(name: &str, selector: Selector) -> Statement {
    Statement::Binding(Binding {
        name: name.into(),
        selector,
        origin: ORIGIN,
    })
}

pub(super) fn iteration(binder: &str, collection: Collection, changes: Vec<Change>) -> Statement {
    Statement::Iteration(Iteration {
        binder: binder.into(),
        collection,
        changes,
        origin: ORIGIN,
    })
}

pub(super) fn body_change(kind: ChangeKind, selector: Selector, payload: Vec<Piece>) -> Change {
    Change {
        kind,
        selector,
        payload,
        origin: ORIGIN,
    }
}

pub(super) fn edit_splices(plan: Plan) -> Result<Vec<Splice>, ()> {
    match plan {
        Plan::Edit(splices) => Ok(splices),
        Plan::File(_) => Err(()),
    }
}

pub(super) fn lower_splices(
    recipe: &Recipe,
    image: &OriginalImage,
    captures: &[CaptureBinding],
) -> Result<Vec<Splice>, CompileDiagnostic> {
    lower(recipe, image, captures, &[]).and_then(|plan| match plan {
        Plan::Edit(splices) => Ok(splices),
        Plan::File(_) => Err(CompileDiagnostic::new(CompileError::Unsupported, ORIGIN)),
    })
}

pub(super) fn render_edit(plan: Plan, original: &[u8]) -> Result<Vec<u8>, ()> {
    let mut splices = edit_splices(plan)?;
    splices.sort_by(|left, right| {
        right
            .target
            .lo()
            .cmp(&left.target.lo())
            .then_with(|| right.target.hi().cmp(&left.target.hi()))
    });
    let mut output = original.to_vec();
    for splice in splices {
        let mut replacement = Vec::new();
        for part in splice.replacement {
            match part {
                Part::Literal(bytes) => replacement.extend_from_slice(&bytes),
                Part::Copy(span) => replacement.extend_from_slice(&original[span.lo()..span.hi()]),
            }
        }
        output.splice(splice.target.lo()..splice.target.hi(), replacement);
    }
    Ok(output)
}

pub(super) fn run_recipe(source: &str, input: &[u8], profile: Profile) -> Result<Vec<u8>, String> {
    let recipe = splicery_syntax::parse_recipe(source.as_bytes(), None)
        .map_err(|error| format!("parse: {error:?}"))?;
    let image = image_with(input, profile).map_err(|()| "image".to_owned())?;
    let plan = lower(&recipe, &image, &[], &[]).map_err(|error| format!("lower: {error:?}"))?;
    render_edit(plan, input).map_err(|()| "not an edit".to_owned())
}
