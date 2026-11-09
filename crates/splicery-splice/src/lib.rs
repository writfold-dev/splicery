//! Splicery's pure byte-splice kernel.
//!
//! Everything in `splice` is plain safe Rust shaped for the Charon/Aeneas
//! translation: no IO, no floats, no `dyn`, no dependencies, `no_std` plus
//! `alloc`. The `proofs` compose gate translates this crate to Lean 4
//! (`proofs/Extracted/`, committed) and proves the kernel theorems over that
//! translation (`proofs/Kernel/`). The proofs are about this code: there is
//! no hand-written Lean copy of these functions. `splicery-kernel` maps the
//! checked domain types onto it and is what production calls.
#![no_std]
#![forbid(unsafe_code)]

extern crate alloc;

pub mod convenience;
pub mod splice;

pub use splice::{
    CAPTURE_BYTES, Cut, Edit, MAX_CANDIDATE_BYTES, MAX_PAYLOAD_BYTES, MAX_SPLICES, Piece, Refusal,
    Rendered, apply_at, prepare, render,
};
