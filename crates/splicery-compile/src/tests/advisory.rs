use super::common::*;
use super::*;
use crate::advisory::{Advisory, AdvisoryKind, line_body_advisories};

const IMPORTS: &[u8] = b"// imports begin\nuse lib::Config;\n// imports end\nuse lib::Config;\n";

fn window_recipe(tail: &str) -> String {
    format!(
        "edit \"sdk/src/lib.rs\" {{\n  @a := one line \"// imports begin\"\n  @b := one line \
         \"// imports end\"\n  @window := between @a and @b\n  @anchor := one line \
         \"use lib::Config;\" within @window\n  {tail}\n}}\n"
    )
}

fn notes(source: &str) -> Result<Vec<Advisory>, String> {
    let recipe = splicery_syntax::parse_recipe(source.as_bytes(), None)
        .map_err(|error| format!("parse: {error:?}"))?;
    Ok(line_body_advisories(&recipe))
}

#[test]
fn after_a_line_body_is_the_spec_meaning_and_joins_lines() -> Result<(), String> {
    // Pins spec/language.md §3 (bare line target selects the body) and §7
    // (`after` inserts at the end of the selection). The result is a known
    // authoring trap, reported by the 2026-11-08 edit pilot; the frontend may
    // not change it without the language contract and surface calculus.
    let joined = window_recipe("after @anchor insert lines like @anchor [\"use lib::Policy;\"]");
    assert_eq!(
        run_recipe(&joined, IMPORTS, Profile::Text)?,
        b"// imports begin\nuse lib::Config;use lib::Policy;\n\n// imports end\nuse lib::Config;\n"
    );
    let before = window_recipe("before @b insert lines like @anchor [\"use lib::Policy;\"]");
    assert_eq!(
        run_recipe(&before, IMPORTS, Profile::Text)?,
        b"// imports begin\nuse lib::Config;\nuse lib::Policy;\n// imports end\nuse lib::Config;\n"
    );
    assert!(notes(&before)?.is_empty());
    let terminated = "edit \"sdk/src/lib.rs\" {\n  @eol := one line \"// imports begin\"\n  \
                      @anchor := one text \"use lib::Config;\\n// imports end\"\n  @first := one \
                      text \"use lib::Config;\\n\" within @anchor\n  after @first insert lines \
                      like @eol [\"use lib::Policy;\"]\n}\n";
    assert_eq!(
        run_recipe(terminated, IMPORTS, Profile::Text)?,
        b"// imports begin\nuse lib::Config;\nuse lib::Policy;\n// imports end\nuse lib::Config;\n"
    );
    assert!(notes(terminated)?.is_empty());
    Ok(())
}

#[test]
fn notes_name_the_change_that_joins_or_blanks_a_line() -> Result<(), String> {
    let joined = window_recipe("after @anchor insert lines like @anchor [\"use lib::Policy;\"]");
    let found = notes(&joined)?;
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].kind, AdvisoryKind::LinesAfterLineBody);
    let statement = "after @anchor insert lines like @anchor [\"use lib::Policy;\"]";
    let start = joined.find(statement).ok_or("statement")?;
    assert_eq!(
        found[0].origin,
        RecipeSpan {
            start,
            end: start + statement.len()
        }
    );
    let rendered = found[0].to_string();
    assert!(rendered.starts_with(&format!(
        "note: recipe bytes [{start},{}): `after` a line binding",
        start + statement.len()
    )));
    assert!(rendered.contains("spec/language.md §3, §7"));

    let deleted = window_recipe("delete @anchor");
    assert_eq!(
        run_recipe(&deleted, IMPORTS, Profile::Text)?,
        b"// imports begin\n\n// imports end\nuse lib::Config;\n"
    );
    let found = notes(&deleted)?;
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].kind, AdvisoryKind::DeleteLineBody);
    assert!(found[0].to_string().contains("leaves an empty line"));

    let direct = "edit \"f\" {\n  @eol := one line \"a\"\n  after one line \"b\" insert lines \
                  like @eol [\"c\"]\n}\n";
    assert_eq!(
        notes(direct)?.first().map(|note| note.kind),
        Some(AdvisoryKind::LinesAfterLineBody)
    );
    let in_iteration = "edit \"f\" {\n  @eol := one line \"a\"\n  each @m in line_matches \
                        rx\"^(?<k>b)$\" expect 1 {\n    after @eol insert lines like @eol \
                        [\"c\"]\n    delete @m.k\n  }\n}\n";
    assert_eq!(
        notes(in_iteration)?
            .iter()
            .map(|note| note.kind)
            .collect::<Vec<_>>(),
        vec![AdvisoryKind::LinesAfterLineBody]
    );
    Ok(())
}

#[test]
fn notes_stay_silent_for_intended_body_edits() -> Result<(), String> {
    for tail in [
        "after @anchor insert \" // pinned\"",
        "after @anchor insert [\"\\n\", \"use lib::Policy;\"]",
        "before @anchor insert lines like @anchor [\"use lib::Policy;\"]",
        "replace @anchor with lines like @anchor [\"use lib::Policy;\"]",
        "after @window insert lines like @anchor [\"use lib::Policy;\"]",
        "@call := one text \"Config\" within @anchor\n  after @call insert \".await\"\n  delete @call",
    ] {
        assert!(notes(&window_recipe(tail))?.is_empty(), "{tail}");
    }
    let group = "edit \"f\" {\n  @eol := one line \"a\"\n  each @m in line_matches \
                 rx\"^(?<k>b)$\" expect 1 {\n    delete @m.k\n  }\n}\n";
    assert!(notes(group)?.is_empty());
    let create = "create \"f\" with \"x\"\n";
    assert!(notes(create)?.is_empty());
    Ok(())
}
