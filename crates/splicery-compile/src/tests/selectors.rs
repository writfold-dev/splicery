use super::common::*;
use super::*;

#[test]
fn one_counts_overlaps_and_honors_within() -> Result<(), ()> {
    let image = image(b"aaa")?;
    let ambiguous = lower_splices(
        &raw_recipe(vec![change(
            ChangeKind::Delete,
            Selector::OneHex {
                needle: b"aa".to_vec(),
                within: None,
            },
            Vec::new(),
        )]),
        &image,
        &[],
    );
    assert_eq!(
        ambiguous,
        Err(CompileDiagnostic {
            kind: CompileError::AmbiguousSelector,
            origin: ORIGIN
        })
    );

    let capture = CaptureBinding {
        name: "tail".to_owned(),
        span: ReadCapture::new(3, 1, 3).ok_or(())?,
    };
    let lowered = lower_splices(
        &raw_recipe(vec![change(
            ChangeKind::Delete,
            Selector::OneHex {
                needle: b"aa".to_vec(),
                within: Some(reference("tail")),
            },
            Vec::new(),
        )]),
        &image,
        &[capture],
    )
    .map_err(|_| ())?;
    assert_eq!((lowered[0].target.lo(), lowered[0].target.hi()), (1, 3));
    Ok(())
}

#[test]
fn bindings_are_source_ordered_and_cannot_shadow() -> Result<(), ()> {
    let image = image(b"abc")?;
    let unknown = lower_splices(
        &raw_recipe(vec![
            change(
                ChangeKind::Delete,
                Selector::Reference(reference("later")),
                Vec::new(),
            ),
            binding("later", range(0, 1)),
        ]),
        &image,
        &[],
    );
    assert_eq!(
        unknown.map(|_| ()),
        Err(CompileDiagnostic {
            kind: CompileError::UnknownBinding,
            origin: ORIGIN
        })
    );

    let capture = CaptureBinding {
        name: "same".to_owned(),
        span: ReadCapture::new(3, 0, 1).ok_or(())?,
    };
    let duplicate = lower_splices(
        &raw_recipe(vec![binding("same", range(1, 2))]),
        &image,
        &[capture],
    );
    assert_eq!(
        duplicate,
        Err(CompileDiagnostic {
            kind: CompileError::DuplicateBinding,
            origin: ORIGIN
        })
    );
    Ok(())
}

#[test]
fn lowers_insertions_and_original_copies() -> Result<(), ()> {
    let image = image(b"abcdef")?;
    let statements = vec![
        binding("left", range(0, 2)),
        change(
            ChangeKind::After,
            range(1, 3),
            vec![
                Piece::Literal(b"X".to_vec()),
                Piece::Keep(reference("left")),
            ],
        ),
        change(
            ChangeKind::Before,
            Selector::Reference(reference("left")),
            vec![Piece::Literal(b"Y".to_vec())],
        ),
    ];
    let lowered = lower_splices(&raw_recipe(statements), &image, &[]).map_err(|_| ())?;
    assert_eq!((lowered[0].target.lo(), lowered[0].target.hi()), (3, 3));
    assert_eq!((lowered[1].target.lo(), lowered[1].target.hi()), (0, 0));
    assert_eq!(
        lowered[0].replacement[1],
        Part::Copy(SourceSpan::new(6, 0, 2).ok_or(())?)
    );
    Ok(())
}

#[test]
fn whole_line_identity_between_and_recorded_eols() -> Result<(), ()> {
    let original = b"a\r\nb\r\nlast";
    let image = text_image(original)?;
    let statements = vec![
        binding(
            "a",
            Selector::OneLine {
                needle: b"a".to_vec(),
                within: None,
            },
        ),
        binding(
            "b",
            Selector::OneLine {
                needle: b"b".to_vec(),
                within: None,
            },
        ),
        change(
            ChangeKind::Replace,
            Selector::Between {
                first: reference("a"),
                second: reference("b"),
            },
            vec![Piece::Literal(b"X".to_vec())],
        ),
    ];
    let plan = lower(&recipe(statements), &image, &[], &[]).map_err(|_| ())?;
    let splices = edit_splices(plan)?;
    assert_eq!((splices[0].target.lo(), splices[0].target.hi()), (3, 3));

    let no_synthetic = lower_splices(
        &recipe(vec![binding(
            "empty",
            Selector::OneLine {
                needle: Vec::new(),
                within: None,
            },
        )]),
        &text_image(b"a\n")?,
        &[],
    );
    assert_eq!(
        no_synthetic.map(|_| ()),
        Err(CompileDiagnostic {
            kind: CompileError::MissingSelector,
            origin: ORIGIN
        })
    );

    let unterminated = lower_splices(
        &recipe(vec![
            binding(
                "last",
                Selector::OneLine {
                    needle: b"last".to_vec(),
                    within: None,
                },
            ),
            change(
                ChangeKind::Replace,
                Selector::Reference(reference("last")),
                vec![Piece::LinesLike {
                    reference: reference("last"),
                    lines: vec![b"x".to_vec()],
                }],
            ),
        ]),
        &image,
        &[],
    );
    assert_eq!(
        unterminated.map(|_| ()),
        Err(CompileDiagnostic {
            kind: CompileError::InvalidCapture,
            origin: ORIGIN
        })
    );

    let partial = lower_splices(
        &recipe(vec![binding(
            "partial",
            Selector::OneLine {
                needle: b"a".to_vec(),
                within: None,
            },
        )]),
        &text_image(b"prefix a\r\n")?,
        &[],
    );
    assert_eq!(
        partial.map(|_| ()),
        Err(CompileDiagnostic {
            kind: CompileError::MissingSelector,
            origin: ORIGIN
        })
    );
    Ok(())
}

#[test]
fn regex_groups_distinguish_absent_from_present_empty() -> Result<(), ()> {
    let present = recipe(vec![iteration(
        "r",
        Collection::LineMatches {
            pattern: "^(?<empty>x*)y$".into(),
            within: None,
            expect: 1,
            origin: ORIGIN,
        },
        vec![body_change(
            ChangeKind::Replace,
            Selector::Reference(reference("r")),
            vec![Piece::Keep(group("r", "empty"))],
        )],
    )]);
    let plan = lower(&present, &text_image(b"y")?, &[], &[]).map_err(|_| ())?;
    assert_eq!(render_edit(plan, b"y")?, b"");

    let absent = recipe(vec![iteration(
        "r",
        Collection::LineMatches {
            pattern: "^(?<first>x)?y$".into(),
            within: None,
            expect: 1,
            origin: ORIGIN,
        },
        vec![body_change(
            ChangeKind::Replace,
            Selector::Reference(reference("r")),
            vec![Piece::Keep(group("r", "first"))],
        )],
    )]);
    assert_eq!(
        lower(&absent, &text_image(b"y")?, &[], &[]).map(|_| ()),
        Err(CompileDiagnostic {
            kind: CompileError::AbsentGroup,
            origin: ORIGIN
        })
    );
    Ok(())
}
