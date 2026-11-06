use super::*;

fn image(bytes: &[u8]) -> Result<OriginalImage, RegexError> {
    OriginalImage::new(bytes.to_vec(), Profile::Text).ok_or(RegexError::InvalidUtf8)
}

fn matched(image: &OriginalImage, pattern: &str, expected: usize) -> Result<FrozenSet, RegexError> {
    match_lines(
        image,
        "rows",
        pattern,
        None,
        expected,
        [7; 32],
        &mut Budget::new(),
    )
}

#[test]
fn posix_classes_are_ascii_inside_and_outside_classes() -> Result<(), RegexError> {
    let image = image(b"\xc3\xa9_9\t\n")?;
    let set = matched(&image, r"\w|\d|\s", 3)?;
    let spans: Vec<_> = set
        .members
        .iter()
        .map(|member| (member.span.lo(), member.span.hi()))
        .collect();
    assert_eq!(spans, [(2, 3), (3, 4), (4, 5)]);
    assert!(set.members.iter().all(|member| member.line.is_none()));
    let class = matched(&image, r"[\w\d\s]+", 1)?;
    assert_eq!(
        (class.members[0].span.lo(), class.members[0].span.hi()),
        (2, 5)
    );
    assert!(!compile_pattern(r"[[:alpha:]\w]")?.is_match("é"));
    Ok(())
}

#[test]
fn dot_consumes_one_unicode_scalar() -> Result<(), RegexError> {
    let set = matched(&image("é\n".as_bytes())?, ".", 1)?;
    assert_eq!((set.members[0].span.lo(), set.members[0].span.hi()), (0, 2));
    Ok(())
}

#[test]
fn groups_distinguish_absent_from_present_empty() -> Result<(), RegexError> {
    let set = matched(&image(b"x\n")?, r"(?<absent>a)?(?<empty>)x", 1)?;
    assert_eq!(set.members[0].groups[0].span, None);
    let empty = set.members[0].groups[1].span;
    assert_eq!(empty.map(|span| (span.lo(), span.hi())), Some((0, 0)));
    Ok(())
}

#[test]
fn inventory_is_nonoverlapping_per_line_and_scoped() -> Result<(), RegexError> {
    let image = image(b"aaa\naa\n")?;
    let set = matched(&image, "aa", 2)?;
    let spans: Vec<_> = set
        .members
        .iter()
        .map(|member| (member.span.lo(), member.span.hi()))
        .collect();
    assert_eq!(spans, [(0, 2), (4, 6)]);
    let span = SourceSpan::new(image.bytes().len(), 4, 6).ok_or(RegexError::InvalidScope)?;
    let scope = ResolvedCapture::plain(span);
    let scoped = match_lines(
        &image,
        "chosen",
        "aa",
        Some(scope),
        1,
        [9; 32],
        &mut Budget::new(),
    )?;
    assert_eq!(scoped.name, "chosen");
    assert_eq!(scoped.matcher_profile, PROFILE_ID);
    assert_eq!(scoped.capture_sha256, [9; 32]);
    assert_eq!(scoped.scope, Some(span));
    assert_eq!(scoped.members[0].line, Some(LineEnding::Lf));
    Ok(())
}

#[test]
fn scope_preserves_physical_anchors_and_actual_eof() -> Result<(), RegexError> {
    let image = image(b"ab cd\nlast")?;
    let middle = SourceSpan::new(image.bytes().len(), 2, 5).ok_or(RegexError::InvalidScope)?;
    let set = match_lines(
        &image,
        "middle",
        r"\w+",
        Some(ResolvedCapture::plain(middle)),
        1,
        [0; 32],
        &mut Budget::new(),
    )?;
    assert_eq!((set.members[0].span.lo(), set.members[0].span.hi()), (3, 5));
    assert_eq!(set.members[0].line, None);
    let anchored = match_lines(
        &image,
        "middle",
        "^cd",
        Some(ResolvedCapture::plain(middle)),
        0,
        [0; 32],
        &mut Budget::new(),
    )?;
    assert!(anchored.members.is_empty());
    let end = SourceSpan::new(image.bytes().len(), 6, 10).ok_or(RegexError::InvalidScope)?;
    let last = match_lines(
        &image,
        "last",
        "last",
        Some(ResolvedCapture::plain(end)),
        1,
        [0; 32],
        &mut Budget::new(),
    )?;
    assert_eq!(last.members[0].line, Some(LineEnding::Unterminated));
    Ok(())
}

#[test]
fn rejects_zero_width_and_unsupported_constructs() -> Result<(), RegexError> {
    assert_eq!(
        matched(&image(b"bbb\n")?, "a*", 0),
        Err(RegexError::ZeroWidthMatch)
    );
    for pattern in [r"(?i)a", r"(?=a)", r"(a)\1", r"(?P<x>a)"] {
        assert!(matches!(
            compile_pattern(pattern),
            Err(RegexError::UnsupportedConstruct)
        ));
    }
    Ok(())
}

#[test]
fn enforces_source_match_and_budget_bounds() -> Result<(), RegexError> {
    assert!(matches!(
        compile_pattern(&"a".repeat(MAX_REGEX_SOURCE_BYTES + 1)),
        Err(RegexError::SourceTooLong)
    ));
    let many = "x\n".repeat(MAX_MATCHES + 1);
    assert_eq!(
        matched(&image(many.as_bytes())?, "x", 0),
        Err(RegexError::TooManyMatches)
    );
    let mut budget = Budget::new();
    let remaining = budget.remaining();
    charge(&mut budget, remaining - 1)?;
    assert_eq!(
        matched_with_budget(&image(b"x\n")?, &mut budget),
        Err(RegexError::BudgetExceeded)
    );
    Ok(())
}

fn matched_with_budget(
    image: &OriginalImage,
    budget: &mut Budget,
) -> Result<FrozenSet, RegexError> {
    match_lines(image, "rows", "x", None, 1, [0; 32], budget)
}
