//! A unified diff derived from the exact splices of a candidate. It is a
//! view: the candidate bytes remain the only thing a host may apply. Each
//! hunk covers the original lines its splices touch, trimmed to the lines
//! that change, with three lines of context; output over 16 KiB refuses
//! rather than truncating.

use splicery_domain::MAX_REPORT_BYTES;
use splicery_kernel::Candidate;

const CONTEXT: usize = 3;
const NO_NEWLINE: &[u8] = b"\n\\ No newline at end of file\n";

/// One splice in original coordinates: `[lo,hi)` becomes `replacement`.
pub struct DiffEdit<'a> {
    pub lo: usize,
    pub hi: usize,
    pub replacement: &'a [u8],
}

/// Original lines `[first, last)` become `new`, a whole number of lines.
struct Change {
    first: usize,
    last: usize,
    new: Vec<u8>,
}

fn split_lines(bytes: &[u8]) -> Vec<&[u8]> {
    bytes.split_inclusive(|byte| *byte == b'\n').collect()
}

/// The 0-based line holding byte `at`, given each line's start offset.
fn line_of(starts: &[usize], at: usize) -> usize {
    starts
        .partition_point(|start| *start <= at)
        .saturating_sub(1)
}

/// Groups splices that share original lines and rebuilds those lines.
fn changes(
    original: &[u8],
    lines: &[&[u8]],
    edits: &[DiffEdit<'_>],
) -> Result<Vec<Change>, String> {
    let mut starts = Vec::with_capacity(lines.len() + 1);
    let mut offset = 0usize;
    for line in lines {
        starts.push(offset);
        offset += line.len();
    }
    let line_starts = starts.clone();
    starts.push(offset);
    let mut blocks: Vec<(usize, usize, Vec<&DiffEdit<'_>>)> = Vec::new();
    let mut previous_hi = 0usize;
    for edit in edits {
        if edit.lo > edit.hi || edit.hi > original.len() || edit.lo < previous_hi {
            return Err("diff splices are not canonical original ranges".to_string());
        }
        previous_hi = edit.hi;
        let after_last_line = original.is_empty() || original.last() == Some(&b'\n');
        let first = if edit.lo == original.len() && after_last_line {
            lines.len()
        } else {
            line_of(&line_starts, edit.lo)
        };
        let last = if edit.hi > edit.lo {
            line_of(&line_starts, edit.hi - 1) + 1
        } else {
            (first + 1).min(lines.len())
        };
        match blocks.last_mut() {
            Some(block) if first < block.1 => {
                block.1 = block.1.max(last);
                block.2.push(edit);
            }
            _ => blocks.push((first, last, vec![edit])),
        }
    }
    let mut result = Vec::with_capacity(blocks.len());
    for (first, last, members) in blocks {
        let mut cursor = starts[first];
        let mut new = Vec::new();
        for edit in members {
            new.extend_from_slice(&original[cursor..edit.lo]);
            new.extend_from_slice(edit.replacement);
            cursor = edit.hi;
        }
        new.extend_from_slice(&original[cursor..starts[last]]);
        result.push(Change { first, last, new });
    }
    Ok(result)
}

/// Drops leading and trailing lines that a change leaves as they were.
fn trim<'a>(change: &'a Change, lines: &[&[u8]]) -> (usize, usize, Vec<&'a [u8]>) {
    let mut new = split_lines(&change.new);
    let (mut first, mut last) = (change.first, change.last);
    let mut skip = 0usize;
    while first < last && new.get(skip) == Some(&lines[first]) {
        skip += 1;
        first += 1;
    }
    new.drain(..skip);
    while first < last && new.last() == Some(&lines[last - 1]) {
        new.pop();
        last -= 1;
    }
    (first, last, new)
}

fn push_line(out: &mut Vec<u8>, marker: u8, line: &[u8]) {
    out.push(marker);
    out.extend_from_slice(line);
    if line.last() != Some(&b'\n') {
        out.extend_from_slice(NO_NEWLINE);
    }
}

fn range(start: usize, count: usize) -> String {
    let first = if count == 0 { start } else { start + 1 };
    format!("{first},{count}")
}

/// A unified diff (`--- a/PATH`, `+++ b/PATH`) of `edits` over `original`.
/// No change yields empty output.
pub fn unified_diff(
    path: &str,
    original: &[u8],
    edits: &[DiffEdit<'_>],
) -> Result<Vec<u8>, String> {
    let lines = split_lines(original);
    let changes = changes(original, &lines, edits)?;
    let trimmed: Vec<_> = changes
        .iter()
        .map(|change| trim(change, &lines))
        .filter(|(first, last, new)| first < last || !new.is_empty())
        .collect();
    let mut out = Vec::new();
    if trimmed.is_empty() {
        return Ok(out);
    }
    out.extend_from_slice(format!("--- a/{path}\n+++ b/{path}\n").as_bytes());
    let mut delta = 0isize;
    let mut index = 0usize;
    while index < trimmed.len() {
        let mut end = index + 1;
        while end < trimmed.len() && trimmed[end].0 <= trimmed[end - 1].1 + 2 * CONTEXT {
            end += 1;
        }
        let start = trimmed[index].0.saturating_sub(CONTEXT);
        let stop = (trimmed[end - 1].1 + CONTEXT).min(lines.len());
        let mut body = Vec::new();
        let mut cursor = start;
        let mut added = 0usize;
        for (first, last, new) in &trimmed[index..end] {
            for line in &lines[cursor..*first] {
                push_line(&mut body, b' ', line);
            }
            for line in &lines[*first..*last] {
                push_line(&mut body, b'-', line);
            }
            for line in new {
                push_line(&mut body, b'+', line);
            }
            added += new.len();
            cursor = *last;
        }
        for line in &lines[cursor..stop] {
            push_line(&mut body, b' ', line);
        }
        let old_count = stop - start;
        let removed: usize = trimmed[index..end].iter().map(|(f, l, _)| l - f).sum();
        let new_count = old_count - removed + added;
        let new_start = start.checked_add_signed(delta).unwrap_or(0);
        out.extend_from_slice(
            format!(
                "@@ -{} +{} @@\n",
                range(start, old_count),
                range(new_start, new_count)
            )
            .as_bytes(),
        );
        out.extend_from_slice(&body);
        delta += added as isize - removed as isize;
        if out.len() > MAX_REPORT_BYTES {
            return Err("diff exceeds the 16 KiB bound; use the candidate or --report".to_string());
        }
        index = end;
    }
    Ok(out)
}

/// The diff of a rendered candidate against the original image it came from.
pub fn candidate_diff(
    path: &str,
    original: &[u8],
    candidate: &Candidate,
) -> Result<Vec<u8>, String> {
    let edits: Vec<DiffEdit<'_>> = candidate
        .edits()
        .iter()
        .map(|edit| DiffEdit {
            lo: edit.target.lo(),
            hi: edit.target.hi(),
            replacement: &edit.replacement,
        })
        .collect();
    unified_diff(path, original, &edits)
}
