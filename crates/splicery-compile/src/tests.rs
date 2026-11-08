use super::*;
use splicery_domain::{CaptureGroup, MAX_MATCHES, Part, Profile, ReadCapture, SourceSpan, Splice};
use splicery_syntax::ast::{
    Binding, Change, ChangeKind, Collection, Iteration, Piece, Reference, Selector,
};

mod advisory;
mod common;
mod file_plan;
mod frozen;
mod payload;
mod selectors;
mod validation;
mod vectors;
