use std::fs;
use std::path::PathBuf;

use super::args::{RecipeSource, View, parse_preview};
use super::preview::{preview, write_out};

fn strings(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_string()).collect()
}

struct Scratch(PathBuf);

impl Scratch {
    fn new(name: &str) -> Result<Self, String> {
        let path = std::env::temp_dir().join(format!("splicery-cli-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(path.join("root/src")).map_err(|error| error.to_string())?;
        Ok(Self(path))
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn preview_views_are_exclusive_and_dash_reads_stdin() -> Result<(), String> {
    let parsed = parse_preview(&strings(&["--diff", "--file", "a", "-"]))?;
    assert_eq!(parsed.view, View::Diff);
    assert!(matches!(parsed.recipe, RecipeSource::Stdin));
    let parsed = parse_preview(&strings(&["--report", "r.spl"]))?;
    assert!(matches!(parsed.recipe, RecipeSource::File(ref path) if path == "r.spl"));
    assert!(parse_preview(&strings(&["--report", "--diff", "r.spl"])).is_err());
    assert!(parse_preview(&strings(&["--out", "a", "--out", "b", "r.spl"])).is_err());
    assert!(parse_preview(&strings(&["--bogus", "r.spl"])).is_err());
    assert!(parse_preview(&strings(&["--diff"])).is_err());
    Ok(())
}

fn run(scratch: &Scratch, recipe: &str, extra: &[&str]) -> Result<Vec<u8>, String> {
    let recipe_path = scratch.0.join("recipe.spl");
    fs::write(&recipe_path, recipe).map_err(|error| error.to_string())?;
    let root = scratch.0.join("root");
    let mut arguments = vec!["--root".to_string(), root.display().to_string()];
    arguments.extend(strings(extra));
    arguments.push(recipe_path.display().to_string());
    preview(&parse_preview(&arguments)?)
}

#[test]
fn diff_and_refusal_context_follow_the_command_line() -> Result<(), String> {
    let scratch = Scratch::new("diff")?;
    let target = scratch.0.join("root/src/log.rs");
    fs::write(&target, "a\nlevel = 1\nb\nlevel = 1\n").map_err(|error| error.to_string())?;
    let diff = run(
        &scratch,
        "@x := one line \"b\"\ndelete whole @x",
        &["--diff", "--file", "src/log.rs"],
    )?;
    assert_eq!(
        String::from_utf8_lossy(&diff),
        "--- a/src/log.rs\n+++ b/src/log.rs\n@@ -1,4 +1,3 @@\n a\n level = 1\n-b\n level = 1\n"
    );
    let ambiguous = "edit \"src/log.rs\" { delete one line \"level = 1\" }";
    let Err(hidden) = run(&scratch, ambiguous, &[]) else {
        return Err("ambiguous selector was accepted".to_string());
    };
    assert!(
        hidden.starts_with("refused: AmbiguousSelector at recipe [20,47) in src/log.rs"),
        "{hidden}"
    );
    assert!(hidden.contains("matched 2 sites: line 2 bytes [2,11); line 4 bytes [14,23)"));
    assert!(!hidden.contains("line 2: level"), "{hidden}");
    let Err(shown) = run(&scratch, ambiguous, &["--file", "src/log.rs"]) else {
        return Err("ambiguous selector was accepted".to_string());
    };
    assert!(
        shown.contains("\n  line 2: level = 1\n  line 4: level = 1\n  hint: "),
        "{shown}"
    );
    assert_eq!(
        fs::read(&target).map_err(|error| error.to_string())?,
        b"a\nlevel = 1\nb\nlevel = 1\n"
    );
    Ok(())
}

#[test]
fn out_creates_a_new_file_outside_the_root_only() -> Result<(), String> {
    let scratch = Scratch::new("out")?;
    let root = scratch.0.join("root");
    let outside = scratch.0.join("candidate.txt");
    write_out(&root, &outside, b"x")?;
    assert_eq!(fs::read(&outside).map_err(|error| error.to_string())?, b"x");
    assert!(write_out(&root, &outside, b"y").is_err());
    assert!(write_out(&root, &root.join("src/new.txt"), b"y").is_err());
    assert!(!root.join("src/new.txt").exists());
    Ok(())
}
