use super::common::*;
use super::*;

fn frozen_set(image: &OriginalImage) -> Result<FrozenSet, ()> {
    let member = ResolvedCapture {
        span: SourceSpan::new(image, 0, 1).ok_or(())?,
        line: None,
        groups: vec![CaptureGroup {
            name: "empty".into(),
            span: Some(SourceSpan::new(image, 0, 0).ok_or(())?),
        }],
    };
    Ok(FrozenSet {
        name: "rows".into(),
        matcher_profile: regex::PROFILE_ID.into(),
        capture_sha256: image.capture_sha256(),
        scope: Some(SourceSpan::new(image, 0, 1).ok_or(())?),
        members: vec![member],
    })
}

fn frozen_iteration() -> Recipe {
    recipe(vec![iteration(
        "r",
        Collection::Frozen(reference("rows")),
        vec![body_change(
            ChangeKind::Replace,
            Selector::Reference(reference("r")),
            vec![Piece::Keep(group("r", "empty"))],
        )],
    )])
}

#[test]
fn frozen_iteration_checks_identity_scope_members_and_binder_scope() -> Result<(), ()> {
    let image = text_image(b"x")?;
    let set = frozen_set(&image)?;
    let plan =
        lower(&frozen_iteration(), &image, &[], std::slice::from_ref(&set)).map_err(|_| ())?;
    assert_eq!(render_edit(plan, b"x")?, b"");

    let mut wrong_hash = set.clone();
    wrong_hash.capture_sha256[0] ^= 1;
    assert!(lower(&frozen_iteration(), &image, &[], &[wrong_hash]).is_err());
    let mut wrong_profile = set.clone();
    wrong_profile.matcher_profile = "other".into();
    assert!(lower(&frozen_iteration(), &image, &[], &[wrong_profile]).is_err());
    let mut wrong_scope = set.clone();
    wrong_scope.scope = Some(SourceSpan::insertion(&image, 0).ok_or(())?);
    assert!(lower(&frozen_iteration(), &image, &[], &[wrong_scope]).is_err());
    let mut wrong_member = set.clone();
    wrong_member.members[0].span = SourceSpan::insertion(&image, 0).ok_or(())?;
    assert!(lower(&frozen_iteration(), &image, &[], &[wrong_member]).is_err());

    let missing = lower(&frozen_iteration(), &image, &[], &[]);
    assert_eq!(
        missing.map(|_| ()),
        Err(CompileDiagnostic {
            kind: CompileError::UnknownBinding,
            origin: ORIGIN
        })
    );
    let duplicate = lower(&frozen_iteration(), &image, &[], &[set.clone(), set]);
    assert_eq!(
        duplicate.map(|_| ()),
        Err(CompileDiagnostic {
            kind: CompileError::DuplicateBinding,
            origin: CAPTURE_ORIGIN,
        })
    );

    let leaked = recipe(vec![
        iteration("r", Collection::Frozen(reference("rows")), Vec::new()),
        change(
            ChangeKind::Delete,
            Selector::Reference(reference("r")),
            Vec::new(),
        ),
    ]);
    let empty_set = FrozenSet {
        members: Vec::new(),
        ..frozen_set(&image)?
    };
    assert_eq!(
        lower(&leaked, &image, &[], &[empty_set]).map(|_| ()),
        Err(CompileDiagnostic {
            kind: CompileError::UnknownBinding,
            origin: ORIGIN
        })
    );
    Ok(())
}
