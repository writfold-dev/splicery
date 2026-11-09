//! Command-line parsing. Values stay typed text here; files are opened only
//! by the preview and inspect commands.

use std::path::PathBuf;

use splicery_wire::parse_nat;

/// What a successful preview writes: exactly one view per invocation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum View {
    Candidate,
    Report,
    Diff,
}

/// Where the recipe comes from: a file, or standard input for `-`.
#[derive(Debug)]
pub(crate) enum RecipeSource {
    File(String),
    Stdin,
}

#[derive(Debug)]
pub(crate) struct PreviewArgs {
    pub(crate) root: PathBuf,
    pub(crate) file: Option<String>,
    pub(crate) captures: Vec<String>,
    pub(crate) frozen: Vec<String>,
    pub(crate) view: View,
    pub(crate) out: Option<PathBuf>,
    pub(crate) recipe: RecipeSource,
}

pub(crate) struct InspectArgs {
    pub(crate) root: PathBuf,
    pub(crate) file: String,
    pub(crate) name: String,
    pub(crate) expect: usize,
    pub(crate) within: Option<String>,
    pub(crate) pattern: String,
}

fn set_view(view: &mut View, chosen: View) -> Result<(), String> {
    if *view != View::Candidate {
        return Err("--report and --diff are exclusive; choose one view".to_string());
    }
    *view = chosen;
    Ok(())
}

pub(crate) fn parse_preview(args: &[String]) -> Result<PreviewArgs, String> {
    let mut root = PathBuf::from(".");
    let mut file = None;
    let mut captures = Vec::new();
    let mut frozen = Vec::new();
    let mut view = View::Candidate;
    let mut out = None;
    let mut recipe = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--root" | "--file" | "--capture" | "--frozen" | "--out" => {
                let option = &args[index];
                let value = args
                    .get(index + 1)
                    .ok_or_else(|| format!("missing value for {option}"))?;
                match option.as_str() {
                    "--root" => root = PathBuf::from(value),
                    "--file" => file = Some(value.clone()),
                    "--frozen" => frozen.push(value.clone()),
                    "--out" if out.is_some() => return Err("--out given twice".to_string()),
                    "--out" => out = Some(PathBuf::from(value)),
                    _ => captures.push(value.clone()),
                }
                index += 2;
            }
            "--report" => {
                set_view(&mut view, View::Report)?;
                index += 1;
            }
            "--diff" => {
                set_view(&mut view, View::Diff)?;
                index += 1;
            }
            option if option.starts_with('-') && option != "-" => {
                return Err(format!("unknown option {option}"));
            }
            value => {
                if recipe.is_some() {
                    return Err("preview accepts exactly one recipe".to_string());
                }
                recipe = Some(if value == "-" {
                    RecipeSource::Stdin
                } else {
                    RecipeSource::File(value.to_string())
                });
                index += 1;
            }
        }
    }
    Ok(PreviewArgs {
        root,
        file,
        captures,
        frozen,
        view,
        out,
        recipe: recipe.ok_or_else(|| "missing recipe (a file, or - for stdin)".to_string())?,
    })
}

pub(crate) fn parse_inspect(args: &[String]) -> Result<InspectArgs, String> {
    let mut root = PathBuf::from(".");
    let mut file = None;
    let mut name = None;
    let mut expect = None;
    let mut within = None;
    let mut pattern = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--root" | "--file" | "--name" | "--expect" | "--within" => {
                let option = &args[index];
                let value = args
                    .get(index + 1)
                    .ok_or_else(|| format!("missing value for {option}"))?;
                match option.as_str() {
                    "--root" => root = PathBuf::from(value),
                    "--file" => file = Some(value.clone()),
                    "--name" => name = Some(value.clone()),
                    "--expect" => {
                        expect = Some(
                            parse_nat(value).ok_or_else(|| "invalid match count".to_string())?,
                        )
                    }
                    _ => within = Some(value.clone()),
                }
                index += 2;
            }
            option if option.starts_with('-') => return Err(format!("unknown option {option}")),
            _ => {
                if pattern.is_some() {
                    return Err("inspect accepts exactly one regex".to_string());
                }
                pattern = Some(args[index].clone());
                index += 1;
            }
        }
    }
    Ok(InspectArgs {
        root,
        file: file.ok_or_else(|| "inspect requires --file".to_string())?,
        name: name.ok_or_else(|| "inspect requires --name".to_string())?,
        expect: expect.ok_or_else(|| "inspect requires --expect".to_string())?,
        within,
        pattern: pattern.ok_or_else(|| "missing regex".to_string())?,
    })
}
