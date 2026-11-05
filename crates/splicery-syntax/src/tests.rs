use super::*;
use crate::ast::{
    Change, ChangeKind, Collection, Iteration, Piece, Recipe, Reference, Selector, Statement,
};
use splicery_domain::{MAX_RECIPE_BYTES, MAX_REGEX_SOURCE_BYTES, RecipeSpan};

fn rejection(source: &[u8]) -> Option<DiagnosticKind> {
    parse_recipe(source, Some("f"))
        .err()
        .map(|error| error.kind)
}

fn nested_indent(depth: usize) -> Vec<u8> {
    let mut source = b"replace @x with ".to_vec();
    for _ in 0..depth {
        source.extend_from_slice(b"indent(\" \",");
    }
    source.extend_from_slice(b"\"x\"");
    source.extend(std::iter::repeat_n(b')', depth));
    source
}

#[test]
fn parses_every_shipped_example() -> Result<(), Diagnostic> {
    let examples: &[(&[u8], &str)] = &[
        (
            include_bytes!("../../../examples/01-await.spl"),
            "sdk/src/client.rs",
        ),
        (
            include_bytes!("../../../examples/02-wrap.spl"),
            "sdk/src/driver.rs",
        ),
        (
            include_bytes!("../../../examples/03-policy.spl"),
            "docs/operations.md",
        ),
        (
            include_bytes!("../../../examples/04-routes.spl"),
            "service/config/routes.conf",
        ),
        (
            include_bytes!("../../../examples/05-swap.spl"),
            "docs/names.txt",
        ),
        (
            include_bytes!("../../../examples/06-raw.spl"),
            "fixtures/frame.bin",
        ),
        (
            include_bytes!("../../../examples/07-lines.spl"),
            "docs/config.ini",
        ),
        (
            include_bytes!("../../../examples/08-indent.spl"),
            "docs/example.txt",
        ),
        (
            include_bytes!("../../../examples/09-import.spl"),
            "sdk/src/lib.rs",
        ),
        (
            include_bytes!("../../../examples/10-move.spl"),
            "cfg/service.conf",
        ),
        (
            include_bytes!("../../../examples/11-absence.spl"),
            "data/table",
        ),
    ];
    for (source, path) in examples {
        let recipe = parse_recipe(source, None)?;
        assert_eq!(recipe.path(), *path);
        assert!(!recipe.statements().is_empty());
    }
    Ok(())
}

#[test]
fn parses_file_lifecycle_operations_and_helpers() -> Result<(), Diagnostic> {
    let create = parse_recipe(b"create \"new\" as bytes with [hex\"00\", \"x\"]", None)?;
    assert!(
        matches!(&create, Recipe::Create { path, raw: true, payload }
        if path == "new" && payload.len() == 2)
    );
    assert_eq!(create.path(), "new");
    assert!(create.raw());
    assert!(create.statements().is_empty());
    let rename = parse_recipe(b"rename \"old\" to \"new\"", None)?;
    assert!(matches!(&rename, Recipe::Rename { source, destination }
        if source == "old" && destination == "new"));
    assert_eq!(rename.path(), "old");
    assert!(!rename.raw());
    let remove = parse_recipe(b"remove \"old\"", None)?;
    assert!(matches!(&remove, Recipe::Remove { path } if path == "old"));
    Ok(())
}

#[test]
fn parses_all_selectors_and_capture_references() -> Result<(), Diagnostic> {
    let source = br#"edit "f" {
@txt := one text "a" within @window.part
@ln := one line "b"
@bytesel := one hex"00ff"
@span := between @txt.left and @ln.right
@offsets := range [0,18446744073709551615)
delete @txt.capture
}"#;
    let recipe = parse_recipe(source, None)?;
    assert!(
        matches!(recipe.statements().first(), Some(Statement::Binding(binding))
        if &source[binding.origin.start..binding.origin.end] == b"@txt := one text \"a\" within @window.part")
    );
    let selectors: Vec<&Selector> = recipe
        .statements()
        .iter()
        .filter_map(|statement| match statement {
            Statement::Binding(binding) => Some(&binding.selector),
            _ => None,
        })
        .collect();
    assert!(
        matches!(selectors.first(), Some(Selector::OneText { within: Some(Reference { name, group }), .. })
        if name == "window" && group.as_deref() == Some("part"))
    );
    assert!(matches!(selectors.get(1), Some(Selector::OneLine { needle, .. }) if needle == b"b"));
    assert!(
        matches!(selectors.get(2), Some(Selector::OneHex { needle, .. }) if needle == &[0, 255])
    );
    assert!(
        matches!(selectors.get(3), Some(Selector::Between { first, second })
        if first.group.as_deref() == Some("left") && second.group.as_deref() == Some("right"))
    );
    assert!(matches!(selectors.get(4), Some(Selector::Range { lo: 0, hi }) if *hi == usize::MAX));
    assert!(
        matches!(recipe.statements().last(), Some(Statement::Change(change))
        if matches!(&change.selector, Selector::Reference(reference)
            if reference.name == "txt" && reference.group.as_deref() == Some("capture")))
    );
    Ok(())
}

#[test]
fn parses_iterations_collections_and_exact_origins() -> Result<(), Diagnostic> {
    let source =
        br#"each @r in line_matches rx"^(?<verb>[A-Z]+)\s+(?<path>/\S+)$" within @w expect 3 {
replace @r with [keep(@r.verb), keep(@r.path)]
}"#;
    let recipe = parse_recipe(source, Some("f"))?;
    let Some(Statement::Iteration(Iteration {
        binder,
        collection,
        changes,
        origin,
    })) = recipe.statements().first()
    else {
        return Err(diagnostic(
            DiagnosticKind::Syntax,
            0,
            0,
            "test expected iteration",
        ));
    };
    assert_eq!(binder, "r");
    assert_eq!(&source[origin.start..origin.end], source);
    assert!(
        matches!(collection, Collection::LineMatches { pattern, within: Some(within), expect: 3, origin }
        if pattern == r"^(?<verb>[A-Z]+)\s+(?<path>/\S+)$" && within.name == "w"
            && &source[origin.start..origin.end] == br#"line_matches rx"^(?<verb>[A-Z]+)\s+(?<path>/\S+)$" within @w expect 3"#)
    );
    assert!(
        matches!(changes.first(), Some(Change { kind: ChangeKind::Replace, payload, .. })
        if matches!(payload.as_slice(), [Piece::Keep(a), Piece::Keep(b)]
            if a.group.as_deref() == Some("verb") && b.group.as_deref() == Some("path")))
    );
    let frozen = parse_recipe(b"each @r in frozen @hits { delete @r.name }", Some("f"))?;
    assert!(
        matches!(frozen.statements().first(), Some(Statement::Iteration(iteration))
        if matches!(&iteration.collection, Collection::Frozen(reference) if reference.name == "hits"))
    );
    Ok(())
}

#[test]
fn parses_recursive_payloads_and_exact_bytes() -> Result<(), Diagnostic> {
    let source = b"replace @x with [hex\"80\", <<'T'\r\na\\#\r\nT\r\n, keep(@x.g), lines like @x [\"A\", \"B\"], indent(\"  \", [\"x\", indent(\"\\t\", keep(@x))])]";
    let recipe = parse_recipe(source, Some("f"))?;
    let Some(Statement::Change(change)) = recipe.statements().first() else {
        return Err(diagnostic(
            DiagnosticKind::Syntax,
            0,
            0,
            "test expected change",
        ));
    };
    assert!(
        matches!(change.payload.as_slice(), [Piece::Literal(hex), Piece::Literal(heredoc), Piece::Keep(_), Piece::LinesLike { lines, .. }, Piece::Indent { prefix, payload, .. }]
        if hex == &[128] && heredoc == b"a\\#\r\n" && lines == &vec![b"A".to_vec(), b"B".to_vec()]
            && prefix == b"  " && matches!(payload.as_slice(), [Piece::Literal(_), Piece::Indent { .. }]))
    );
    assert_eq!(
        change.origin,
        RecipeSpan {
            start: 0,
            end: source.len()
        }
    );
    assert!(parse_recipe(b"replace @x with []", Some("f")).is_ok());
    Ok(())
}

#[test]
fn accepts_grammar_fixtures_and_strict_comments() {
    for source in [
        &b"@call := one text \"call()\"\nafter @call insert \".await\""[..],
        &b"delete range [0,0)"[..],
        &b"create \"api/README.md\" with \"hello\\n\""[..],
        &b"rename \"api/old\" to \"api/new\""[..],
        &b"remove \"api/a.txt\""[..],
        &b"each @r in frozen @hits { replace @r.name with \"ok\" }"[..],
        &b"replace @x with []"[..],
        &b"edit \"r/x\" { # comment\n @window := range [0,2) }"[..],
        &b"after @a insert <<'X'\r\n# raw $x\r\nX"[..],
    ] {
        let interactive = if source.starts_with(b"edit ") {
            None
        } else {
            Some("f")
        };
        assert!(parse_recipe(source, interactive).is_ok(), "{source:?}");
    }
    assert!(parse_recipe(b"edit \"x\"\r\n{\r\ndelete @x\r\n}", None).is_ok());
}

#[test]
fn rejects_retired_truncated_and_trailing_syntax() {
    for source in [
        &b"let @call = one text \"x\""[..],
        &b"@call = one text \"x\""[..],
        &b"@a := one text"[..],
        &b"replace @a with hex\"abc\""[..],
        &b"edit \"r/f\" { delete range [01,2) }"[..],
        &b"replace @a with \"\\u1234\""[..],
        &b"edit \"r/x\" { each @a in line_matches rx\"x\" { delete @a } }"[..],
        &b"replace @x with <<'T'\nunterminated"[..],
        &b"delete @one"[..],
        &b"edit \"a\" {"[..],
        &b"edit \"r/f\" {} junk"[..],
        &b"replace @x with [\"a\",]"[..],
        &b"replace @x with lines like @x [\"a\",]"[..],
        &b"replace @x with lines like @x []"[..],
        &b"each @r in frozen @hits { @x := range [0,0) }"[..],
        &b"each @r in frozen @hits { each @x in frozen @hits {} }"[..],
    ] {
        assert_eq!(
            rejection(source),
            Some(DiagnosticKind::Syntax),
            "{source:?}"
        );
    }
    assert_eq!(rejection(b""), Some(DiagnosticKind::Syntax));
    assert!(parse_recipe(b"delete range [0,0)", None).is_err());
    assert!(parse_recipe(b"edit \"a\" {}", Some("b")).is_err());
}

#[test]
fn enforces_encoding_nul_and_line_ending_rules() {
    assert_eq!(
        rejection(b"replace @x with \"\xff\""),
        Some(DiagnosticKind::Encoding)
    );
    assert_eq!(
        rejection(b"\xef\xbb\xbfremove \"x\""),
        Some(DiagnosticKind::Encoding)
    );
    assert_eq!(
        rejection(b"replace @x with \"\xef\xbb\xbf\""),
        Some(DiagnosticKind::Encoding)
    );
    assert_eq!(rejection(b"remove \"x\"\0"), Some(DiagnosticKind::Syntax));
    assert_eq!(
        rejection(b"# comment\0\nremove \"x\""),
        Some(DiagnosticKind::Syntax)
    );
    assert_eq!(rejection(b"remove \"x\"\r"), Some(DiagnosticKind::Syntax));
    assert_eq!(
        rejection(b"replace @x with \"a\nb\""),
        Some(DiagnosticKind::Syntax)
    );
    assert_eq!(
        rejection("replace @x with \"\u{85}\"".as_bytes()),
        Some(DiagnosticKind::Syntax)
    );
    let nul = parse_recipe(b"replace @x with <<'N'\n\0\nN", Some("f"));
    assert!(matches!(nul, Ok(Recipe::Edit { statements, .. })
        if matches!(statements.first(), Some(Statement::Change(change))
            if matches!(change.payload.as_slice(), [Piece::Literal(bytes)] if bytes == b"\0\n"))));
}

#[test]
fn enforces_string_hex_and_heredoc_rules() -> Result<(), Diagnostic> {
    let escaped = parse_recipe(
        "create \"café\" with \"\\\"\\\\\\n\\r\\tcafé\"".as_bytes(),
        None,
    )?;
    assert!(matches!(escaped, Recipe::Create { payload, .. }
        if matches!(payload.as_slice(), [Piece::Literal(bytes)] if bytes == "\"\\\n\r\tcafé".as_bytes())));
    assert!(parse_recipe(b"replace @x with hex\"\"", Some("f")).is_ok());
    assert_eq!(
        rejection(b"replace @x with hex\"0g\""),
        Some(DiagnosticKind::Syntax)
    );
    for (source, expected) in [
        (
            &b"replace @x with <<'X'\r\na\r\nb\n#c\nX\r\n"[..],
            &b"a\r\nb\n#c\n"[..],
        ),
        (&b"replace @x with <<'X'\nbody\nX"[..], &b"body\n"[..]),
        (&b"replace @x with <<'X'\nX"[..], &b""[..]),
        (&b"replace @x with <<'X'\na\rb\nX"[..], &b"a\rb\n"[..]),
    ] {
        let recipe = parse_recipe(source, Some("f"))?;
        assert!(
            matches!(recipe.statements().first(), Some(Statement::Change(change))
            if matches!(change.payload.as_slice(), [Piece::Literal(bytes)] if bytes == expected))
        );
    }
    assert_eq!(
        rejection(b"replace @x with <<'X'"),
        Some(DiagnosticKind::Syntax)
    );
    assert_eq!(
        rejection(b"replace @x with <<'X'\rbody"),
        Some(DiagnosticKind::Syntax)
    );
    assert_eq!(
        rejection(b"replace @x with <<'X'\nbody\nXX"),
        Some(DiagnosticKind::Syntax)
    );
    assert_eq!(
        rejection(b"replace @x with <<'X'\nfirst\nX\njunk"),
        Some(DiagnosticKind::Syntax)
    );
    let long_tag = [b"replace @x with <<'".as_slice(), &[b'A'; 65], b"'\n"].concat();
    assert_eq!(rejection(&long_tag), Some(DiagnosticKind::Limit));
    Ok(())
}

#[test]
fn enforces_reserved_names_and_preserves_raw_regex() -> Result<(), Diagnostic> {
    for word in ["edit", "expect", "line_matches", "rx", "indent"] {
        let handle = format!("@{word} := one text \"x\"");
        assert_eq!(rejection(handle.as_bytes()), Some(DiagnosticKind::Syntax));
        let reference = format!("delete @x.{word}");
        assert_eq!(
            rejection(reference.as_bytes()),
            Some(DiagnosticKind::Syntax)
        );
        let pattern =
            format!("each @r in line_matches rx\"(?<{word}>x)\" expect 1 {{ delete @r }}");
        assert_eq!(rejection(pattern.as_bytes()), Some(DiagnosticKind::Syntax));
    }
    let source =
        br#"each @r in line_matches rx"(?<verb>\S+\x22)[(?<expect>)]" expect 0 { delete @r.verb }"#;
    let recipe = parse_recipe(source, Some("f"))?;
    assert!(
        matches!(recipe.statements().first(), Some(Statement::Iteration(iteration))
        if matches!(&iteration.collection, Collection::LineMatches { pattern, .. }
            if pattern == r"(?<verb>\S+\x22)[(?<expect>)]"))
    );
    assert_eq!(
        rejection(b"each @r in line_matches rx\"a\nb\" expect 0 { delete @r }"),
        Some(DiagnosticKind::Syntax)
    );
    Ok(())
}

#[test]
fn enforces_regex_nat_recipe_and_depth_limits() {
    let accepted = format!(
        "each @r in line_matches rx\"{}\" expect 0 {{ delete @r }}",
        "a".repeat(MAX_REGEX_SOURCE_BYTES)
    );
    assert!(parse_recipe(accepted.as_bytes(), Some("f")).is_ok());
    let excessive = format!(
        "each @r in line_matches rx\"{}\" expect 0 {{ delete @r }}",
        "a".repeat(MAX_REGEX_SOURCE_BYTES + 1)
    );
    assert_eq!(rejection(excessive.as_bytes()), Some(DiagnosticKind::Limit));
    let overflow = format!("delete range [{},0)", usize::MAX as u128 + 1);
    assert_eq!(rejection(overflow.as_bytes()), Some(DiagnosticKind::Limit));
    let huge = [b"delete range [".as_slice(), &vec![b'9'; 4096], b",0)"].concat();
    assert_eq!(rejection(&huge), Some(DiagnosticKind::Limit));
    assert!(parse_recipe(&nested_indent(32), Some("f")).is_ok());
    assert_eq!(rejection(&nested_indent(33)), Some(DiagnosticKind::Limit));
    assert_eq!(rejection(&nested_indent(128)), Some(DiagnosticKind::Limit));
    assert_eq!(
        rejection(&vec![b' '; MAX_RECIPE_BYTES + 1]),
        Some(DiagnosticKind::Limit)
    );
}
