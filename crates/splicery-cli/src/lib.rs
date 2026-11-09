use std::ffi::OsString;
use std::io::{self, Write};

use splicery_kernel::apply_at;
use splicery_wire::{hex, parse_hex, parse_nat};

mod args;
mod preview;
mod refusal;

use args::{InspectArgs, PreviewArgs, parse_inspect, parse_preview};

const USAGE: &str = "splicery preview [--root DIR] [--file PATH] [--capture @name=LO:HI] [--frozen @name=FILE] [--report | --diff] [--out NEW_FILE] RECIPE|-\n\
splicery inspect [--root DIR] --file PATH --name @handle --expect N [--within LO:HI] REGEX\n\
splicery kernel apply LO WIDTH INPUT_HEX REPLACEMENT_HEX\n";

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

fn parse_kernel(args: &[String]) -> Command {
    if args.first().map(String::as_str) != Some("apply") || args.len() != 5 {
        return Command::Refusal;
    }
    match (
        parse_nat(&args[1]),
        parse_nat(&args[2]),
        parse_hex(&args[3]),
        parse_hex(&args[4]),
    ) {
        (Some(lo), Some(width), Some(input), Some(replacement)) => Command::Apply {
            lo,
            width,
            input,
            replacement,
        },
        _ => Command::Refusal,
    }
}

fn parse_command(args: Vec<String>) -> Result<Command, String> {
    match args.first().map(String::as_str) {
        Some("preview") => parse_preview(&args[1..]).map(Command::Preview),
        Some("inspect") => parse_inspect(&args[1..]).map(Command::Inspect),
        Some("kernel") => Ok(parse_kernel(&args[1..])),
        _ => Err(USAGE.to_string()),
    }
}

fn run(command: Command) -> Result<(Vec<u8>, u8), String> {
    match command {
        Command::Refusal => Ok((b"REFUSAL\n".to_vec(), 1)),
        Command::Preview(args) => {
            let bytes = preview::preview(&args)?;
            match &args.out {
                Some(out) => preview::write_out(&args.root, out, &bytes).map(|()| (Vec::new(), 0)),
                None => Ok((bytes, 0)),
            }
        }
        Command::Inspect(args) => preview::inspect(args).map(|bytes| (bytes, 0)),
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

#[cfg(test)]
mod tests;
