use splicery_domain::{FilePlan, MAX_REPORT_BYTES};

use super::{hex, json_string};

const CREATE_PREFIX: &str =
    "{\"schema_version\":1,\"scope\":\"offline-plan\",\"op\":\"create\",\"path\":";
const CREATE_MIDDLE: &str = ",\"content_hex\":\"";
const RENAME_PREFIX: &str =
    "{\"schema_version\":1,\"scope\":\"offline-plan\",\"op\":\"rename\",\"source\":";
const RENAME_MIDDLE: &str = ",\"destination\":";
const REMOVE_PREFIX: &str =
    "{\"schema_version\":1,\"scope\":\"offline-plan\",\"op\":\"remove\",\"path\":";
const SUFFIX: &str = "}";
const QUOTED_SUFFIX: &str = "\"}";

fn quoted_len(value: &str) -> Option<usize> {
    value.chars().try_fold(2usize, |length, character| {
        let encoded = match character {
            '"' | '\\' | '\n' | '\r' | '\t' => 2,
            c if c < ' ' => 6,
            c => c.len_utf8(),
        };
        length.checked_add(encoded)
    })
}

fn create_len(path: &str, content: &[u8]) -> Option<usize> {
    let fixed = CREATE_PREFIX.len() + CREATE_MIDDLE.len() + QUOTED_SUFFIX.len();
    quoted_len(path)?
        .checked_add(content.len().checked_mul(2)?)?
        .checked_add(fixed)
}

fn rename_len(source: &str, destination: &str) -> Option<usize> {
    let fixed = RENAME_PREFIX.len() + RENAME_MIDDLE.len() + SUFFIX.len();
    quoted_len(source)?
        .checked_add(quoted_len(destination)?)?
        .checked_add(fixed)
}

fn remove_len(path: &str) -> Option<usize> {
    quoted_len(path)?.checked_add(REMOVE_PREFIX.len() + SUFFIX.len())
}

fn file_plan_len(plan: &FilePlan) -> Option<usize> {
    match plan {
        FilePlan::Create { path, content } => create_len(path, content),
        FilePlan::Rename {
            source,
            destination,
        } => rename_len(source, destination),
        FilePlan::Remove { path } => remove_len(path),
    }
}

/// Renders one inert file operation. Admission and filesystem effects remain host concerns.
pub fn encode_file_plan(plan: &FilePlan) -> Result<String, String> {
    let length =
        file_plan_len(plan).ok_or_else(|| "file plan length overflows usize".to_string())?;
    if length > MAX_REPORT_BYTES {
        return Err("exact file plan exceeds the 16 KiB bound".to_string());
    }
    let mut out = String::with_capacity(length);
    match plan {
        FilePlan::Create { path, content } => {
            out.push_str(CREATE_PREFIX);
            out.push_str(&json_string(path));
            out.push_str(CREATE_MIDDLE);
            out.push_str(&hex(content));
            out.push_str(QUOTED_SUFFIX);
        }
        FilePlan::Rename {
            source,
            destination,
        } => {
            out.push_str(RENAME_PREFIX);
            out.push_str(&json_string(source));
            out.push_str(RENAME_MIDDLE);
            out.push_str(&json_string(destination));
            out.push_str(SUFFIX);
        }
        FilePlan::Remove { path } => {
            out.push_str(REMOVE_PREFIX);
            out.push_str(&json_string(path));
            out.push_str(SUFFIX);
        }
    }
    Ok(out)
}
