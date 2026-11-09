use super::*;
use crate::ast::{Reference, Selector, Statement};

fn selectors(source: &[u8]) -> Result<Vec<Selector>, Diagnostic> {
    let recipe = parse_recipe(source, Some("f"))?;
    Ok(recipe
        .statements()
        .iter()
        .filter_map(|statement| match statement {
            Statement::Binding(binding) => Some(binding.selector.clone()),
            Statement::Change(change) => Some(change.selector.clone()),
            Statement::Iteration(_) => None,
        })
        .collect())
}

fn named(name: &str, group: Option<&str>) -> Reference {
    Reference {
        name: name.to_owned(),
        group: group.map(str::to_owned),
    }
}

#[test]
fn parses_whole_and_from_to_selectors() -> Result<(), Diagnostic> {
    let parsed = selectors(
        b"@span := from @open.left to @close\nafter whole @row insert \"x\"\ndelete whole @r.g",
    )?;
    assert_eq!(
        parsed,
        vec![
            Selector::FromTo {
                first: named("open", Some("left")),
                second: named("close", None),
            },
            Selector::Whole(named("row", None)),
            Selector::Whole(named("r", Some("g"))),
        ]
    );
    Ok(())
}

#[test]
fn whole_and_from_to_require_references_and_reserve_their_words() {
    for source in [
        b"delete whole one line \"x\"".as_slice(),
        b"delete whole",
        b"delete from @a @b",
        b"delete from @a to",
        b"delete from @a and @b",
        b"@whole := one text \"x\"",
        b"@from := one text \"x\"",
        b"delete @x.whole",
    ] {
        assert_eq!(
            parse_recipe(source, Some("f"))
                .err()
                .map(|error| error.kind),
            Some(DiagnosticKind::Syntax),
            "{}",
            String::from_utf8_lossy(source)
        );
    }
}
