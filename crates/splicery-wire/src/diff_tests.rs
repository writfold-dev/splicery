use super::diff::{DiffEdit, unified_diff};

fn diff(original: &str, edits: &[(usize, usize, &str)]) -> Result<String, String> {
    let edits: Vec<DiffEdit<'_>> = edits
        .iter()
        .map(|(lo, hi, text)| DiffEdit {
            lo: *lo,
            hi: *hi,
            replacement: text.as_bytes(),
        })
        .collect();
    let bytes = unified_diff("f.rs", original.as_bytes(), &edits)?;
    String::from_utf8(bytes).map_err(|error| error.to_string())
}

#[test]
fn diff_trims_to_changed_lines_with_context() -> Result<(), String> {
    let original = "1\n2\n3\n4\n5\n6\n7\n8\n9\n";
    assert_eq!(
        diff(original, &[(8, 9, "five")])?,
        "--- a/f.rs\n+++ b/f.rs\n@@ -2,7 +2,7 @@\n 2\n 3\n 4\n-5\n+five\n 6\n 7\n 8\n"
    );
    // Inserting a whole line at a line start shows only the new line.
    assert_eq!(
        diff(original, &[(2, 2, "x\n")])?,
        "--- a/f.rs\n+++ b/f.rs\n@@ -1,4 +1,5 @@\n 1\n+x\n 2\n 3\n 4\n"
    );
    assert_eq!(diff(original, &[(0, 2, "1\n")])?, "");
    Ok(())
}

#[test]
fn diff_separates_distant_hunks_and_tracks_offsets() -> Result<(), String> {
    let original: String = (1..=20).map(|n| format!("{n}\n")).collect();
    let second = original.find("18\n").ok_or("fixture")?;
    let text = diff(&original, &[(0, 2, ""), (second, second, "17.5\n")])?;
    assert_eq!(
        text,
        "--- a/f.rs\n+++ b/f.rs\n@@ -1,4 +1,3 @@\n-1\n 2\n 3\n 4\n\
         @@ -15,6 +14,7 @@\n 15\n 16\n 17\n+17.5\n 18\n 19\n 20\n"
    );
    Ok(())
}

#[test]
fn diff_marks_missing_final_newlines_and_eof_insertions() -> Result<(), String> {
    assert_eq!(
        diff("a\nb", &[(3, 3, "c")])?,
        "--- a/f.rs\n+++ b/f.rs\n@@ -1,2 +1,2 @@\n a\n-b\n\\ No newline at end of file\n+bc\n\\ No newline at end of file\n"
    );
    assert_eq!(
        diff("a\n", &[(2, 2, "b\n")])?,
        "--- a/f.rs\n+++ b/f.rs\n@@ -1,1 +1,2 @@\n a\n+b\n"
    );
    assert_eq!(
        diff("", &[(0, 0, "x\n")])?,
        "--- a/f.rs\n+++ b/f.rs\n@@ -0,0 +1,1 @@\n+x\n"
    );
    Ok(())
}

#[test]
fn diff_refuses_unbounded_output_and_bad_ranges() {
    let original = "x\n".repeat(20_000);
    let replacement = "y\n".repeat(20_000);
    assert!(diff(&original, &[(0, original.len(), &replacement)]).is_err());
    assert!(diff("ab", &[(1, 3, "")]).is_err());
    assert!(diff("ab", &[(1, 2, ""), (0, 1, "")]).is_err());
}
