use super::*;
use crate::ast::{Piece, Recipe, Statement};
use splicery_domain::RecipeSpan;

#[test]
fn parses_scoped_edits_and_exact_payloads() -> Result<(), Diagnostic> {
    let source = b"edit \"f\" as bytes { # c\r\n@x := one hex\"00ff\" within @w\nreplace @x with [hex\"80\", <<'T'\r\na\\#\r\nT\r\n, keep(@x)] }";
    let recipe = parse_recipe(source, None)?;
    assert!(recipe.raw && recipe.path == "f" && recipe.statements.len() == 2);
    assert!(
        matches!(recipe.statements.first(), Some(Statement::Binding(binding))
        if source.get(binding.origin.start..binding.origin.end) == Some(&b"@x := one hex\"00ff\" within @w"[..]))
    );
    assert!(
        matches!(recipe.statements.get(1), Some(Statement::Change(change))
        if change.origin.end == source.len() - 2 && matches!(change.payload.as_slice(),
            [Piece::Literal(a), Piece::Literal(b), Piece::Keep(name)]
            if a == &[0x80] && b == b"a\\#\r\n" && name == "x"))
    );
    Ok(())
}

#[test]
fn parses_fragment_and_string_escapes() {
    let result = parse_recipe(b"after @a insert \"x\\n\\t\\r\\\\\\\"\"", Some("f"));
    assert!(
        matches!(result, Ok(Recipe { path, statements, .. }) if path == "f" && statements.len() == 1)
    );
}

#[test]
fn rejects_invalid_and_unsupported_forms() {
    assert!(matches!(
        parse_recipe(b"\xef\xbb\xbfdelete range [0,0)", None),
        Err(Diagnostic {
            kind: DiagnosticKind::Encoding,
            origin: RecipeSpan { start: 0, end: 3 },
            ..
        })
    ));
    for (source, kind) in [
        (&b"delete @a\r"[..], DiagnosticKind::Syntax),
        (&b"@a = one text \"x\""[..], DiagnosticKind::Syntax),
        (&b"replace @a with hex\"0\""[..], DiagnosticKind::Syntax),
        (
            &b"delete between @a and @b"[..],
            DiagnosticKind::Unsupported,
        ),
        (&b"create \"f\" with \"x\""[..], DiagnosticKind::Unsupported),
        (&b"each @x in frozen @x {}"[..], DiagnosticKind::Unsupported),
    ] {
        assert_eq!(
            parse_recipe(source, Some("f"))
                .map(|_| ())
                .map_err(|error| error.kind),
            Err(kind)
        );
    }
    assert!(
        parse_recipe(b"delete range [01,0)", Some("f")).is_err()
            && parse_recipe(b"delete range [0,0) junk", Some("f")).is_err()
            && parse_recipe(b"delete range [0,0)", None).is_err()
    );
}
