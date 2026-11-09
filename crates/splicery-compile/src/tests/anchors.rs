use super::common::*;
use super::*;
use crate::detail::DetailReason;

fn refusal(source: &str, input: &[u8]) -> Result<CompileDiagnostic, String> {
    let recipe = splicery_syntax::parse_recipe(source.as_bytes(), None)
        .map_err(|error| format!("parse: {error:?}"))?;
    let image = text_image(input).map_err(|()| "image".to_owned())?;
    match lower(&recipe, &image, &[], &[]) {
        Ok(_) => Err("lowered".to_owned()),
        Err(diagnostic) => Ok(diagnostic),
    }
}

type Sites = (DetailReason, usize, Vec<(usize, usize)>);

fn sites(diagnostic: &CompileDiagnostic) -> Option<Sites> {
    diagnostic.detail.as_ref().map(|detail| {
        let spans = detail.recorded().map(|span| (span.lo(), span.hi()));
        (detail.reason, detail.count, spans.collect())
    })
}

#[test]
fn whole_selects_the_line_with_its_terminator() -> Result<(), String> {
    let edit = |body: &str, input: &[u8]| {
        run_recipe(&format!("edit \"f\" {{ {body} }}"), input, Profile::Text)
    };
    let lf = "@b := one line \"b\" delete whole @b";
    assert_eq!(edit(lf, b"a\nb\nc")?, b"a\nc");
    let crlf = "@a := one line \"a\" after whole @a insert lines like @a [\"x\"]";
    assert_eq!(edit(crlf, b"a\r\nb\r\n")?, b"a\r\nx\r\nb\r\n");
    // An unterminated final line: whole selects its body through EOF.
    assert_eq!(edit(lf, b"a\nb")?, b"a\n");
    let bound = "@a := one line \"a\" @w := whole @a replace @w with keep(@w)";
    assert_eq!(edit(bound, b"a\r\n")?, b"a\r\n");
    let plain = refusal(
        "edit \"f\" { @t := one text \"a\" delete whole @t }",
        b"a\n",
    )?;
    assert_eq!(plain.kind, CompileError::InvalidCapture);
    Ok(())
}

#[test]
fn from_to_spans_both_anchors_and_refuses_misordered_ones() -> Result<(), String> {
    let source = "edit \"f\" { @o := one text \"f(\" @c := one text \");\" \
                  @s := from @o to @c replace @s with [\"g(\", keep(@s), \")\"] }";
    assert_eq!(
        run_recipe(source, b"x = f(\n  1\n);\n", Profile::Text)?,
        b"x = g(f(\n  1\n);)\n"
    );
    let adjacent = "edit \"f\" { @a := range [0,1) @b := range [1,2) delete from @a to @b }";
    assert_eq!(run_recipe(adjacent, b"abc", Profile::Text)?, b"c");
    for (anchors, expected) in [
        ("@a := range [2,3) @b := range [0,1)", vec![(2, 3), (0, 1)]),
        ("@a := range [0,2) @b := range [1,3)", vec![(0, 2), (1, 3)]),
        ("@a := range [1,1) @b := range [1,1)", vec![(1, 1), (1, 1)]),
    ] {
        let source = format!("edit \"f\" {{ {anchors} delete from @a to @b }}");
        let refused = refusal(&source, b"abc")?;
        assert_eq!(refused.kind, CompileError::MissingSelector);
        assert_eq!(sites(&refused), Some((DetailReason::Anchors, 2, expected)));
    }
    Ok(())
}

#[test]
fn missing_selectors_report_bounded_near_misses() -> Result<(), String> {
    let trimmed = refusal("edit \"f\" { delete one line \"b\" }", b"a\n  b\t\nb2\n")?;
    assert_eq!(trimmed.kind, CompileError::MissingSelector);
    assert_eq!(
        sites(&trimmed),
        Some((DetailReason::Whitespace, 1, vec![(2, 6)]))
    );
    let eol = refusal("edit \"f\" { delete one text \"a\\nb\" }", b"a\r\nb")?;
    assert_eq!(
        sites(&eol),
        Some((DetailReason::LineEnding, 1, vec![(0, 4)]))
    );
    let scoped = "edit \"f\" { @w := range [0,2) delete one text \"zz\" within @w }";
    assert_eq!(
        sites(&refusal(scoped, b"abzz")?),
        Some((DetailReason::OutsideWithin, 1, vec![(2, 4)]))
    );
    let none = refusal("edit \"f\" { delete one text \"q\" }", b"abc")?;
    assert_eq!(
        (none.kind, none.detail),
        (CompileError::MissingSelector, None)
    );
    let many = refusal("edit \"f\" { delete one text \"a\" }", b"aaaaaa")?;
    assert_eq!(
        sites(&many),
        Some((
            DetailReason::Matches,
            6,
            vec![(0, 1), (1, 2), (2, 3), (3, 4)]
        ))
    );
    Ok(())
}
