use super::common::*;
use super::*;

#[test]
fn rejects_empty_needles_bad_ranges_excess_matches_and_delete_payload() -> Result<(), ()> {
    let image = image(&vec![b'a'; MAX_MATCHES + 1])?;
    let empty = lower_splices(
        &raw_recipe(vec![change(
            ChangeKind::Delete,
            Selector::OneHex {
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
            origin: ORIGIN
        })
    );
    let excessive = lower_splices(
        &raw_recipe(vec![change(
            ChangeKind::Delete,
            Selector::OneHex {
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
            origin: ORIGIN
        })
    );
    assert_eq!(
        lower_splices(
            &raw_recipe(vec![change(ChangeKind::Delete, range(3, 2), vec![])]),
            &image,
            &[],
        )
        .map(|_| ()),
        Err(CompileDiagnostic {
            kind: CompileError::OutOfBounds,
            origin: ORIGIN
        })
    );
    assert_eq!(
        lower_splices(
            &raw_recipe(vec![change(
                ChangeKind::Delete,
                range(0, 1),
                vec![Piece::Literal(b"x".to_vec())],
            )]),
            &image,
            &[],
        )
        .map(|_| ()),
        Err(CompileDiagnostic {
            kind: CompileError::Unsupported,
            origin: ORIGIN
        })
    );
    Ok(())
}

#[test]
fn rejects_foreign_capture_spans() -> Result<(), ()> {
    let image = image_with(b"abc", Profile::Raw)?;
    let longer_image = image_with(b"abcd", Profile::Raw)?;
    let capture = CaptureBinding {
        name: "foreign".to_owned(),
        span: ReadCapture::new(&longer_image, 0, 4).ok_or(())?,
    };
    assert_eq!(
        lower(&raw_recipe(Vec::new()), &image, &[capture], &[]),
        Err(CompileDiagnostic {
            kind: CompileError::OutOfBounds,
            origin: CAPTURE_ORIGIN,
        })
    );

    let same_length_image = image_with(b"xyz", Profile::Raw)?;
    let capture = CaptureBinding {
        name: "foreign".to_owned(),
        span: ReadCapture::new(&same_length_image, 0, 3).ok_or(())?,
    };
    assert_eq!(
        lower(&raw_recipe(Vec::new()), &image, &[capture], &[]),
        Err(CompileDiagnostic {
            kind: CompileError::OutOfBounds,
            origin: CAPTURE_ORIGIN,
        })
    );
    Ok(())
}
