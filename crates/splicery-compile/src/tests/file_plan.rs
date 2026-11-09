use super::common::*;
use super::*;

#[test]
fn file_operations_are_separate_plans_and_create_is_literal_only() -> Result<(), ()> {
    let image = text_image(b"ignored")?;
    let create = Recipe::Create {
        path: "new.txt".into(),
        raw: false,
        payload: vec![
            Piece::Literal(b"first\n".to_vec()),
            Piece::Literal(b"second".to_vec()),
        ],
    };
    assert_eq!(
        lower(&create, &image, &[], &[]).map_err(|_| ())?,
        Plan::File(FilePlan::Create {
            path: "new.txt".into(),
            content: b"first\nsecond".to_vec(),
        })
    );
    let unsupported = Recipe::Create {
        path: "new.txt".into(),
        raw: false,
        payload: vec![Piece::Keep(reference("x"))],
    };
    assert_eq!(
        lower(&unsupported, &image, &[], &[]).map(|_| ()),
        Err(CompileDiagnostic {
            kind: CompileError::Unsupported,
            origin: CAPTURE_ORIGIN,
            detail: None
        })
    );
    let invalid_text = Recipe::Create {
        path: "new.txt".into(),
        raw: false,
        payload: vec![Piece::Literal(vec![0xff])],
    };
    assert_eq!(
        lower(&invalid_text, &image, &[], &[]).map(|_| ()),
        Err(CompileDiagnostic {
            kind: CompileError::WrongProfile,
            origin: CAPTURE_ORIGIN,
            detail: None
        })
    );

    let rename = Recipe::Rename {
        source: "a/old".into(),
        destination: "a/new".into(),
    };
    assert_eq!(
        lower(&rename, &image, &[], &[]).map_err(|_| ())?,
        Plan::File(FilePlan::Rename {
            source: "a/old".into(),
            destination: "a/new".into()
        })
    );
    let remove = Recipe::Remove {
        path: "a/old".into(),
    };
    assert_eq!(
        lower(&remove, &image, &[], &[]).map_err(|_| ())?,
        Plan::File(FilePlan::Remove {
            path: "a/old".into()
        })
    );
    Ok(())
}
