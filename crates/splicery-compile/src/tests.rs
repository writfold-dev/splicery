use super::*;
use splicery_domain::Profile;
use splicery_syntax::ast::{Binding, Change};

const ORIGIN: RecipeSpan = RecipeSpan { start: 2, end: 7 };

fn recipe(statements: Vec<Statement>) -> Recipe {
    Recipe {
        path: String::new(),
        raw: true,
        statements,
    }
}

fn image(bytes: &[u8]) -> Result<OriginalImage, ()> {
    OriginalImage::new(bytes.to_vec(), Profile::Raw).ok_or(())
}

fn range(lo: usize, hi: usize) -> Selector {
    Selector::Range { lo, hi }
}

fn change(kind: ChangeKind, selector: Selector, payload: Vec<Piece>) -> Statement {
    Statement::Change(Change {
        kind,
        selector,
        payload,
        origin: ORIGIN,
    })
}

fn binding(name: &str, selector: Selector) -> Statement {
    Statement::Binding(Binding {
        name: name.to_owned(),
        selector,
        origin: ORIGIN,
    })
}

#[test]
fn one_counts_overlaps_and_honors_within() -> Result<(), ()> {
    let image = image(b"aaa")?;
    let ambiguous = lower(
        &recipe(vec![change(
            ChangeKind::Delete,
            Selector::OneText {
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
            origin: ORIGIN,
        })
    );

    let capture = CaptureBinding {
        name: "tail".to_owned(),
        span: SourceSpan::new(3, 1, 3).ok_or(())?,
    };
    let lowered = lower(
        &recipe(vec![change(
            ChangeKind::Delete,
            Selector::OneHex {
                needle: b"aa".to_vec(),
                within: Some("tail".into()),
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
    let unknown = lower(
        &recipe(vec![
            change(
                ChangeKind::Delete,
                Selector::Reference("later".into()),
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
            origin: ORIGIN,
        })
    );

    let capture = CaptureBinding {
        name: "same".to_owned(),
        span: SourceSpan::new(3, 0, 1).ok_or(())?,
    };
    let duplicate = lower(
        &recipe(vec![binding("same", range(1, 2))]),
        &image,
        &[capture],
    );
    assert_eq!(
        duplicate,
        Err(CompileDiagnostic {
            kind: CompileError::DuplicateBinding,
            origin: ORIGIN,
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
            vec![Piece::Literal(b"X".to_vec()), Piece::Keep("left".into())],
        ),
        change(
            ChangeKind::Before,
            Selector::Reference("left".into()),
            vec![Piece::Literal(b"Y".to_vec())],
        ),
    ];
    let lowered = lower(&recipe(statements), &image, &[]).map_err(|_| ())?;
    assert_eq!((lowered[0].target.lo(), lowered[0].target.hi()), (3, 3));
    assert_eq!((lowered[1].target.lo(), lowered[1].target.hi()), (0, 0));
    assert_eq!(
        lowered[0].replacement[1],
        Part::Copy(SourceSpan::new(6, 0, 2).ok_or(())?,)
    );
    Ok(())
}

#[test]
fn rejects_empty_needles_bad_ranges_and_excess_matches() -> Result<(), ()> {
    let image = image(&vec![b'a'; MAX_MATCHES + 1])?;
    let empty = lower(
        &recipe(vec![change(
            ChangeKind::Delete,
            Selector::OneText {
                needle: Vec::new(),
                within: None,
            },
            Vec::new(),
        )]),
        &image,
        &[],
    );
    assert_eq!(
        empty.map(|_| ()),
        Err(CompileDiagnostic {
            kind: CompileError::MissingSelector,
            origin: ORIGIN,
        })
    );

    let excessive = lower(
        &recipe(vec![change(
            ChangeKind::Delete,
            Selector::OneText {
                needle: vec![b'a'],
                within: None,
            },
            Vec::new(),
        )]),
        &image,
        &[],
    );
    assert_eq!(
        excessive.map(|_| ()),
        Err(CompileDiagnostic {
            kind: CompileError::TooManyMatches,
            origin: ORIGIN,
        })
    );
    assert_eq!(
        lower(
            &recipe(vec![change(ChangeKind::Delete, range(3, 2), vec![])]),
            &image,
            &[]
        )
        .map(|_| ()),
        Err(CompileDiagnostic {
            kind: CompileError::OutOfBounds,
            origin: ORIGIN
        }),
    );
    Ok(())
}

#[test]
fn rejects_payload_on_delete_and_foreign_capture_spans() -> Result<(), ()> {
    let image = image(b"abc")?;
    let unsupported = lower(
        &recipe(vec![change(
            ChangeKind::Delete,
            range(0, 1),
            vec![Piece::Literal(b"x".to_vec())],
        )]),
        &image,
        &[],
    );
    assert_eq!(
        unsupported.map(|_| ()),
        Err(CompileDiagnostic {
            kind: CompileError::Unsupported,
            origin: ORIGIN,
        })
    );

    let capture = CaptureBinding {
        name: "foreign".to_owned(),
        span: SourceSpan::new(4, 0, 4).ok_or(())?,
    };
    let foreign = lower(&recipe(Vec::new()), &image, &[capture]);
    assert_eq!(
        foreign,
        Err(CompileDiagnostic {
            kind: CompileError::OutOfBounds,
            origin: CAPTURE_ORIGIN,
        })
    );
    Ok(())
}
