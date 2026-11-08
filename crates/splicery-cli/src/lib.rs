use std::ffi::OsString;
use std::fs;
use std::io::{self, Write};
use std::path::{Component, Path, PathBuf};

use splicery_compile::{advisory::line_body_advisories, lower, regex::match_lines};
use splicery_domain::{
    Budget, CaptureBinding, FrozenSet, MAX_INPUT_BYTES, OriginalImage, Plan, Profile, ReadCapture,
    ResolvedCapture, SourceSpan,
};
use splicery_kernel::{PreparedBatch, apply_at, render};
use splicery_syntax::ast::Recipe;
use splicery_syntax::parse_recipe;
use splicery_wire::{
    encode_file_plan, encode_frozen, hex, parse_frozen, parse_hex, parse_nat, report,
};

const USAGE: &str = "splicery preview [--root DIR] [--file PATH] [--capture @name=LO:HI] [--frozen @name=FILE] [--report] RECIPE\n\
splicery inspect [--root DIR] --file PATH --name @handle --expect N [--within LO:HI] REGEX\n\
splicery kernel apply LO WIDTH INPUT_HEX REPLACEMENT_HEX\n";

struct PreviewArgs {
    root: PathBuf,
    file: Option<String>,
    captures: Vec<String>,
    frozen: Vec<String>,
    report: bool,
    recipe: String,
}

struct InspectArgs {
    root: PathBuf,
    file: String,
    name: String,
    expect: usize,
    within: Option<String>,
    pattern: String,
}

enum Command {
    Preview(PreviewArgs),
    Inspect(InspectArgs),
    Apply {
        lo: usize,
        width: usize,
        input: Vec<u8>,
        replacement: Vec<u8>,
    },
    Refusal,
}

fn text_args(args: impl Iterator<Item = OsString>) -> Result<Vec<String>, String> {
    args.skip(1)
        .map(|arg| {
            arg.into_string()
                .map_err(|_| "arguments must be UTF-8".to_string())
        })
        .collect()
}

fn parse_preview(args: &[String]) -> Result<PreviewArgs, String> {
    let mut root = PathBuf::from(".");
    let mut file = None;
    let mut captures = Vec::new();
    let mut frozen = Vec::new();
    let mut report = false;
    let mut recipe = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--root" | "--file" | "--capture" | "--frozen" => {
                let option = &args[index];
                let value = args
                    .get(index + 1)
                    .ok_or_else(|| format!("missing value for {option}"))?;
                match option.as_str() {
                    "--root" => root = PathBuf::from(value),
                    "--file" => file = Some(value.clone()),
                    "--frozen" => frozen.push(value.clone()),
                    _ => captures.push(value.clone()),
                }
                index += 2;
            }
            "--report" => {
                report = true;
                index += 1;
            }
            option if option.starts_with('-') => return Err(format!("unknown option {option}")),
            _ => {
                if recipe.is_some() {
                    return Err("preview accepts exactly one recipe".to_string());
                }
                recipe = Some(args[index].clone());
                index += 1;
            }
        }
    }
    Ok(PreviewArgs {
        root,
        file,
        captures,
        frozen,
        report,
        recipe: recipe.ok_or_else(|| "missing recipe".to_string())?,
    })
}

fn parse_inspect(args: &[String]) -> Result<InspectArgs, String> {
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

fn parse_command(args: Vec<String>) -> Result<Command, String> {
    let Some(first) = args.first() else {
        return Err(USAGE.to_string());
    };
    if first == "preview" {
        return parse_preview(&args[1..]).map(Command::Preview);
    }
    if first == "inspect" {
        return parse_inspect(&args[1..]).map(Command::Inspect);
    }
    if first == "kernel" {
        if args.get(1).map(String::as_str) != Some("apply") || args.len() != 6 {
            return Ok(Command::Refusal);
        }
        let Some(lo) = parse_nat(&args[2]) else {
            return Ok(Command::Refusal);
        };
        let Some(width) = parse_nat(&args[3]) else {
            return Ok(Command::Refusal);
        };
        let Some(input) = parse_hex(&args[4]) else {
            return Ok(Command::Refusal);
        };
        let Some(replacement) = parse_hex(&args[5]) else {
            return Ok(Command::Refusal);
        };
        return Ok(Command::Apply {
            lo,
            width,
            input,
            replacement,
        });
    }
    Err(USAGE.to_string())
}

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

fn preview(args: PreviewArgs) -> Result<Vec<u8>, String> {
    let source = fs::read(&args.recipe).map_err(|error| format!("cannot read recipe: {error}"))?;
    let recipe =
        parse_recipe(&source, args.file.as_deref()).map_err(|error| format!("{error:?}"))?;
    match &recipe {
        Recipe::Edit { raw, .. } => {
            for note in line_body_advisories(&recipe) {
                let _ = writeln!(io::stderr().lock(), "{note}");
            }
            let profile = if *raw { Profile::Raw } else { Profile::Text };
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
            let plan =
                lower(&recipe, &image, &captures, &frozen).map_err(|error| format!("{error:?}"))?;
            let Plan::Edit(splices) = plan else {
                return Err("file plan returned for an edit recipe".to_string());
            };
            let batch = PreparedBatch::new(image, splices).map_err(|error| format!("{error:?}"))?;
            let candidate = render(&batch).map_err(|error| format!("{error:?}"))?;
            if args.report {
                return report(recipe.path(), &candidate).map(String::into_bytes);
            }
            Ok(candidate.into_bytes())
        }
        Recipe::Create { raw, .. } => {
            let image =
                OriginalImage::new(Vec::new(), if *raw { Profile::Raw } else { Profile::Text })
                    .ok_or_else(|| "invalid create profile".to_string())?;
            let plan = lower(&recipe, &image, &[], &[]).map_err(|error| format!("{error:?}"))?;
            let Plan::File(file_plan) = plan else {
                return Err("edit plan returned for a create recipe".to_string());
            };
            encode_file_plan(&file_plan).map(String::into_bytes)
        }
        Recipe::Rename { .. } | Recipe::Remove { .. } => {
            let image = OriginalImage::new(Vec::new(), Profile::Text)
                .ok_or_else(|| "invalid file operation profile".to_string())?;
            let plan = lower(&recipe, &image, &[], &[]).map_err(|error| format!("{error:?}"))?;
            let Plan::File(file_plan) = plan else {
                return Err("edit plan returned for a file operation".to_string());
            };
            encode_file_plan(&file_plan).map(String::into_bytes)
        }
    }
}

fn read_image(root: &Path, relative: &str, profile: Profile) -> Result<OriginalImage, String> {
    let target = contained_file(root, relative)?;
    let metadata =
        fs::metadata(&target).map_err(|error| format!("cannot inspect target: {error}"))?;
    if metadata.len() > MAX_INPUT_BYTES as u64 {
        return Err("target exceeds the 8 MiB input bound".to_string());
    }
    let bytes = fs::read(&target).map_err(|error| format!("cannot read target: {error}"))?;
    OriginalImage::new(bytes, profile).ok_or_else(|| "invalid input profile or size".to_string())
}

fn inspect(args: InspectArgs) -> Result<Vec<u8>, String> {
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

fn run(command: Command) -> Result<(Vec<u8>, u8), String> {
    match command {
        Command::Refusal => Ok((b"REFUSAL\n".to_vec(), 1)),
        Command::Preview(args) => preview(args).map(|bytes| (bytes, 0)),
        Command::Inspect(args) => inspect(args).map(|bytes| (bytes, 0)),
        Command::Apply {
            lo,
            width,
            input,
            replacement,
        } => {
            if input.len() > 4096 || replacement.len() > 4096 {
                return Ok((b"REFUSAL\n".to_vec(), 1));
            }
            match apply_at(&input, lo, width, &replacement) {
                Some(bytes) => Ok((format!("{}\n", hex(&bytes)).into_bytes(), 0)),
                None => Ok((b"REFUSAL\n".to_vec(), 1)),
            }
        }
    }
}

pub fn main(args: impl Iterator<Item = OsString>) -> u8 {
    let result = text_args(args).and_then(parse_command).and_then(run);
    match result {
        Ok((bytes, code)) => {
            let mut stdout = io::stdout().lock();
            if let Err(error) = stdout.write_all(&bytes).and_then(|_| stdout.flush()) {
                let _ = writeln!(io::stderr().lock(), "output failed: {error}");
                return 1;
            }
            code
        }
        Err(message) => {
            let _ = writeln!(io::stderr().lock(), "{message}");
            2
        }
    }
}
