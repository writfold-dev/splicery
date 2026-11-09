//! `preview` and `inspect`: contained reads of one regular file, then pure
//! compile/kernel work. Neither command writes the target; `--out` creates a
//! new file outside the preview root and never replaces an existing one.

use std::fs::{self, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Component, Path, PathBuf};

use splicery_compile::{advisory::line_body_advisories, lower, regex::match_lines};
use splicery_domain::{
    Budget, CaptureBinding, FrozenSet, MAX_INPUT_BYTES, MAX_RECIPE_BYTES, OriginalImage, Plan,
    Profile, ReadCapture, ResolvedCapture, SourceSpan,
};
use splicery_kernel::{PreparedBatch, render};
use splicery_syntax::ast::Recipe;
use splicery_syntax::parse_recipe;
use splicery_wire::{
    candidate_diff, encode_file_plan, encode_frozen, parse_frozen, parse_nat, report,
};

use crate::args::{InspectArgs, PreviewArgs, RecipeSource, View};
use crate::refusal::{self, Target};

fn contained_file(root: &Path, relative: &str) -> Result<PathBuf, String> {
    let path = Path::new(relative);
    if relative.is_empty()
        || path
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
    {
        return Err("target must be a nonempty relative path without dot segments".to_string());
    }
    let root = fs::canonicalize(root).map_err(|error| format!("cannot resolve root: {error}"))?;
    if !root.is_dir() {
        return Err("preview root is not a directory".to_string());
    }
    let candidate = fs::canonicalize(root.join(path))
        .map_err(|error| format!("cannot resolve target: {error}"))?;
    if !candidate.starts_with(&root) {
        return Err("target escapes the preview root".to_string());
    }
    let metadata = fs::symlink_metadata(&candidate)
        .map_err(|error| format!("cannot inspect target: {error}"))?;
    if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
        return Err("target must be a regular non-symlink file".to_string());
    }
    Ok(candidate)
}

fn capture(text: &str, image: &OriginalImage) -> Result<CaptureBinding, String> {
    let (name, range) = text
        .split_once('=')
        .ok_or_else(|| "capture must be @name=LO:HI".to_string())?;
    let (lo, hi) = range
        .split_once(':')
        .ok_or_else(|| "capture must be @name=LO:HI".to_string())?;
    if !name.starts_with('@') || name.len() < 2 {
        return Err("capture name must be a handle".to_string());
    }
    let lo = parse_nat(lo).ok_or_else(|| "invalid capture start".to_string())?;
    let hi = parse_nat(hi).ok_or_else(|| "invalid capture end".to_string())?;
    let span = ReadCapture::new(image, lo, hi)
        .ok_or_else(|| "capture is outside the image".to_string())?;
    Ok(CaptureBinding {
        name: name[1..].to_string(),
        span,
    })
}

fn frozen_input(text: &str, image: &OriginalImage) -> Result<FrozenSet, String> {
    let (alias, filename) = text
        .split_once('=')
        .ok_or_else(|| "frozen input must be @name=FILE".to_string())?;
    if !alias.starts_with('@') || alias.len() < 2 {
        return Err("frozen name must be a handle".to_string());
    }
    let metadata =
        fs::metadata(filename).map_err(|error| format!("cannot inspect frozen input: {error}"))?;
    if metadata.len() > 1024 * 1024 {
        return Err("frozen input exceeds 1 MiB".to_string());
    }
    let bytes = fs::read(filename).map_err(|error| format!("cannot read frozen input: {error}"))?;
    let set = parse_frozen(&bytes, image)?;
    if set.name != alias[1..] {
        return Err("frozen alias does not match the embedded name".to_string());
    }
    Ok(set)
}

/// Reads at most one byte past the recipe limit, so the parser refuses an
/// oversized recipe with its usual `Limit` diagnostic.
fn read_recipe(source: &RecipeSource) -> Result<Vec<u8>, String> {
    let limit = u64::try_from(MAX_RECIPE_BYTES + 1).unwrap_or(u64::MAX);
    let mut bytes = Vec::new();
    match source {
        RecipeSource::Stdin => io::stdin()
            .lock()
            .take(limit)
            .read_to_end(&mut bytes)
            .map_err(|error| format!("cannot read recipe from stdin: {error}"))?,
        RecipeSource::File(path) => fs::File::open(path)
            .and_then(|file| file.take(limit).read_to_end(&mut bytes))
            .map_err(|error| format!("cannot read recipe: {error}"))?,
    };
    Ok(bytes)
}

pub(crate) fn read_image(
    root: &Path,
    relative: &str,
    profile: Profile,
) -> Result<OriginalImage, String> {
    let target = contained_file(root, relative)?;
    let metadata =
        fs::metadata(&target).map_err(|error| format!("cannot inspect target: {error}"))?;
    if metadata.len() > MAX_INPUT_BYTES as u64 {
        return Err("target exceeds the 8 MiB input bound".to_string());
    }
    let bytes = fs::read(&target).map_err(|error| format!("cannot read target: {error}"))?;
    OriginalImage::new(bytes, profile).ok_or_else(|| "invalid input profile or size".to_string())
}

fn preview_edit(args: &PreviewArgs, recipe: &Recipe, raw: bool) -> Result<Vec<u8>, String> {
    for note in line_body_advisories(recipe) {
        let _ = writeln!(io::stderr().lock(), "{note}");
    }
    let profile = if raw { Profile::Raw } else { Profile::Text };
    let image = read_image(&args.root, recipe.path(), profile)?;
    let captures = args
        .captures
        .iter()
        .map(|value| capture(value, &image))
        .collect::<Result<Vec<_>, _>>()?;
    let frozen = args
        .frozen
        .iter()
        .map(|value| frozen_input(value, &image))
        .collect::<Result<Vec<_>, _>>()?;
    let plan = lower(recipe, &image, &captures, &frozen).map_err(|diagnostic| {
        let target = Target {
            path: recipe.path(),
            bytes: image.bytes(),
            show_context: args.file.is_some(),
        };
        refusal::compile(&diagnostic, &target)
    })?;
    let Plan::Edit(splices) = plan else {
        return Err("file plan returned for an edit recipe".to_string());
    };
    let original = image.bytes().to_vec();
    let batch = PreparedBatch::new(image, splices)
        .map_err(|error| refusal::kernel(error, recipe.path()))?;
    let candidate = render(&batch).map_err(|error| refusal::kernel(error, recipe.path()))?;
    match args.view {
        View::Candidate => Ok(candidate.into_bytes()),
        View::Report => report(recipe.path(), &candidate).map(String::into_bytes),
        View::Diff => candidate_diff(recipe.path(), &original, &candidate),
    }
}

fn file_plan(recipe: &Recipe, profile: Profile) -> Result<Vec<u8>, String> {
    let image = OriginalImage::new(Vec::new(), profile)
        .ok_or_else(|| "invalid file operation profile".to_string())?;
    let plan = lower(recipe, &image, &[], &[]).map_err(|error| format!("{error:?}"))?;
    let Plan::File(file_plan) = plan else {
        return Err("edit plan returned for a file operation".to_string());
    };
    encode_file_plan(&file_plan).map(String::into_bytes)
}

pub(crate) fn preview(args: &PreviewArgs) -> Result<Vec<u8>, String> {
    let source = read_recipe(&args.recipe)?;
    let recipe = parse_recipe(&source, args.file.as_deref())
        .map_err(|diagnostic| refusal::syntax(&diagnostic))?;
    match &recipe {
        Recipe::Edit { raw, .. } => preview_edit(args, &recipe, *raw),
        Recipe::Create { raw, .. } => {
            file_plan(&recipe, if *raw { Profile::Raw } else { Profile::Text })
        }
        Recipe::Rename { .. } | Recipe::Remove { .. } => file_plan(&recipe, Profile::Text),
    }
}

/// Writes the preview output to a new file that is outside the preview root.
pub(crate) fn write_out(root: &Path, out: &Path, bytes: &[u8]) -> Result<(), String> {
    let root = fs::canonicalize(root).map_err(|error| format!("cannot resolve root: {error}"))?;
    let name = out
        .file_name()
        .ok_or_else(|| "--out must name a file".to_string())?;
    let parent = match out.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent,
        _ => Path::new("."),
    };
    let parent = fs::canonicalize(parent)
        .map_err(|error| format!("cannot resolve --out directory: {error}"))?;
    if parent.starts_with(&root) {
        return Err(
            "--out must be outside the preview root; applying belongs to the host".to_string(),
        );
    }
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(parent.join(name))
        .map_err(|error| format!("cannot create --out file (it must not exist): {error}"))?;
    file.write_all(bytes)
        .and_then(|()| file.sync_all())
        .map_err(|error| format!("cannot write --out file: {error}"))
}

pub(crate) fn inspect(args: InspectArgs) -> Result<Vec<u8>, String> {
    if !args.name.starts_with('@') || args.name.len() < 2 {
        return Err("inspect name must be a handle".to_string());
    }
    let image = read_image(&args.root, &args.file, Profile::Text)?;
    let scope = match args.within {
        Some(range) => {
            let (lo, hi) = range
                .split_once(':')
                .ok_or_else(|| "scope must be LO:HI".to_string())?;
            let lo = parse_nat(lo).ok_or_else(|| "invalid scope start".to_string())?;
            let hi = parse_nat(hi).ok_or_else(|| "invalid scope end".to_string())?;
            let span = SourceSpan::new(&image, lo, hi)
                .ok_or_else(|| "scope is outside the image".to_string())?;
            Some(ResolvedCapture::plain(span))
        }
        None => None,
    };
    let mut budget = Budget::new();
    let set = match_lines(
        &image,
        &args.name[1..],
        &args.pattern,
        scope,
        args.expect,
        image.capture_sha256(),
        &mut budget,
    )
    .map_err(|error| format!("{error:?}"))?;
    encode_frozen(&set).map(String::into_bytes)
}
