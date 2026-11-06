use super::common::*;
use super::*;

#[test]
fn payload_forms_preserve_bytes_and_indent_only_nonempty_bodies() -> Result<(), ()> {
    let original = b"abc";
    let image = text_image(original)?;
    let statements = vec![change(
        ChangeKind::Replace,
        range(0, 3),
        vec![Piece::Indent {
            prefix: b"  ".to_vec(),
            payload: vec![
                Piece::Literal(b"a\r\n\n \t\nbc".to_vec()),
                Piece::Keep(reference("missing")),
            ],
        }],
    )];
    let missing = lower(&recipe(statements), &image, &[], &[]);
    assert_eq!(
        missing.map(|_| ()),
        Err(CompileDiagnostic {
            kind: CompileError::UnknownBinding,
            origin: ORIGIN
        })
    );

    let indented = recipe(vec![change(
        ChangeKind::Replace,
        range(0, 3),
        vec![Piece::Indent {
            prefix: b"  ".to_vec(),
            payload: vec![Piece::Literal(b"a\r\n\n \t\nbc".to_vec())],
        }],
    )]);
    let plan = lower(&indented, &image, &[], &[]).map_err(|_| ())?;
    assert_eq!(render_edit(plan, original)?, b"  a\r\n\n   \t\n  bc");

    let plan = lower(
        &raw_recipe(vec![
            binding("all", range(0, 3)),
            change(
                ChangeKind::Replace,
                range(0, 3),
                vec![
                    Piece::Literal(b"string\n".to_vec()),
                    Piece::Literal(b"heredoc\r\n".to_vec()),
                    Piece::Literal(vec![0, 0xff]),
                    Piece::Keep(reference("all")),
                ],
            ),
        ]),
        &self::image(original)?,
        &[],
        &[],
    )
    .map_err(|_| ())?;
    assert_eq!(
        render_edit(plan, original)?,
        b"string\nheredoc\r\n\x00\xffabc"
    );
    Ok(())
}

#[test]
fn raw_restrictions_and_text_boundaries_are_enforced() -> Result<(), ()> {
    let raw = self::image(&[0xff, b'a'])?;
    let one_text = raw_recipe(vec![binding(
        "a",
        Selector::OneText {
            needle: b"a".to_vec(),
            within: None,
        },
    )]);
    assert_eq!(
        lower(&one_text, &raw, &[], &[]).map(|_| ()),
        Err(CompileDiagnostic {
            kind: CompileError::WrongProfile,
            origin: ORIGIN
        })
    );
    let raw_fragment = recipe(vec![change(
        ChangeKind::Delete,
        Selector::OneHex {
            needle: vec![0xff],
            within: None,
        },
        Vec::new(),
    )]);
    assert!(lower(&raw_fragment, &raw, &[], &[]).is_ok());
    let indent = raw_recipe(vec![change(
        ChangeKind::Replace,
        range(0, 1),
        vec![Piece::Indent {
            prefix: b" ".to_vec(),
            payload: Vec::new(),
        }],
    )]);
    assert_eq!(
        lower(&indent, &raw, &[], &[]).map(|_| ()),
        Err(CompileDiagnostic {
            kind: CompileError::WrongProfile,
            origin: ORIGIN
        })
    );

    let text = text_image("é".as_bytes())?;
    let split = recipe(vec![change(ChangeKind::Delete, range(0, 1), Vec::new())]);
    assert_eq!(
        lower(&split, &text, &[], &[]).map(|_| ()),
        Err(CompileDiagnostic {
            kind: CompileError::WrongProfile,
            origin: ORIGIN
        })
    );
    let split_literal = recipe(vec![change(
        ChangeKind::Replace,
        range(0, 2),
        vec![Piece::Literal(vec![0xc3]), Piece::Literal(vec![0xa9])],
    )]);
    let plan = lower(&split_literal, &text, &[], &[]).map_err(|_| ())?;
    assert_eq!(render_edit(plan, "é".as_bytes())?, "é".as_bytes());
    let invalid_literal = recipe(vec![change(
        ChangeKind::Replace,
        range(0, 2),
        vec![Piece::Literal(vec![0xff])],
    )]);
    assert_eq!(
        lower(&invalid_literal, &text, &[], &[]).map(|_| ()),
        Err(CompileDiagnostic {
            kind: CompileError::WrongProfile,
            origin: ORIGIN
        })
    );
    Ok(())
}
