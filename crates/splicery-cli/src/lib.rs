use std::ffi::OsString;
use std::fs;
use std::io::{self, Write};
use std::path::{Component, Path, PathBuf};

use splicery_compile::lower;
use splicery_domain::{CaptureBinding, MAX_INPUT_BYTES, OriginalImage, Profile, SourceSpan};
use splicery_kernel::{PreparedBatch, apply_at, render};
use splicery_syntax::parse_recipe;
use splicery_wire::{hex, parse_hex, parse_nat, report};

const USAGE: &str = "splicery preview [--root DIR] [--file PATH] [--capture @name=LO:HI] [--report] RECIPE\n\
splicery kernel apply LO WIDTH INPUT_HEX REPLACEMENT_HEX\n";

struct PreviewArgs {
    root: PathBuf,
    file: Option<String>,
    captures: Vec<String>,
    report: bool,
    recipe: String,
}

enum Command {
    Preview(PreviewArgs),
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
    let mut report = false;
    let mut recipe = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--root" | "--file" | "--capture" => {
                let option = &args[index];
                let value = args
                    .get(index + 1)
                    .ok_or_else(|| format!("missing value for {option}"))?;
                match option.as_str() {
                    "--root" => root = PathBuf::from(value),
                    "--file" => file = Some(value.clone()),
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
        report,
        recipe: recipe.ok_or_else(|| "missing recipe".to_string())?,
    })
}

fn parse_command(args: Vec<String>) -> Result<Command, String> {
    let Some(first) = args.first() else {
        return Err(USAGE.to_string());
    };
    if first == "preview" {
        return parse_preview(&args[1..]).map(Command::Preview);
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

fn capture(text: &str, image_len: usize) -> Result<CaptureBinding, String> {
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
    let span = SourceSpan::new(image_len, lo, hi)
        .ok_or_else(|| "capture is outside the image".to_string())?;
    Ok(CaptureBinding {
        name: name[1..].to_string(),
        span,
    })
}

fn preview(args: PreviewArgs) -> Result<Vec<u8>, String> {
    let source = fs::read(&args.recipe).map_err(|error| format!("cannot read recipe: {error}"))?;
    let recipe =
        parse_recipe(&source, args.file.as_deref()).map_err(|error| format!("{error:?}"))?;
    let target = contained_file(&args.root, &recipe.path)?;
    let metadata =
        fs::metadata(&target).map_err(|error| format!("cannot inspect target: {error}"))?;
    if metadata.len() > MAX_INPUT_BYTES as u64 {
        return Err("target exceeds the 8 MiB input bound".to_string());
    }
    let bytes = fs::read(&target).map_err(|error| format!("cannot read target: {error}"))?;
    let profile = if recipe.raw {
        Profile::Raw
    } else {
        Profile::Text
    };
    let image = OriginalImage::new(bytes, profile)
        .ok_or_else(|| "invalid input profile or size".to_string())?;
    let captures = args
        .captures
        .iter()
        .map(|value| capture(value, image.bytes().len()))
        .collect::<Result<Vec<_>, _>>()?;
    let splices = lower(&recipe, &image, &captures).map_err(|error| format!("{error:?}"))?;
    let batch = PreparedBatch::new(image, splices).map_err(|error| format!("{error:?}"))?;
    let candidate = render(&batch).map_err(|error| format!("{error:?}"))?;
    if args.report {
        return report(&recipe.path, &candidate).map(String::into_bytes);
    }
    Ok(candidate.into_bytes())
}

fn run(command: Command) -> Result<(Vec<u8>, u8), String> {
    match command {
        Command::Refusal => Ok((b"REFUSAL\n".to_vec(), 1)),
        Command::Preview(args) => preview(args).map(|bytes| (bytes, 0)),
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
