//! Production conveniences for the kernel types, written by hand.
//!
//! Charon translates every impl in a module it starts from, derived or not,
//! and a derived `Debug` drags `core::fmt` into the translation as an
//! axiomatized external. These impls live outside the translated module
//! (`[package.metadata.charon] start_from` names only `splice`), so none of
//! them is part of the proved surface. `PartialEq` for `Cut` delegates to the
//! kernel's own `same_capture`, so `==` and the proved identity agree.
use crate::splice::{Cut, Edit, Piece, Refusal, same_capture};
use core::fmt;

impl Clone for Cut {
    fn clone(&self) -> Self {
        Self {
            capture: self.capture,
            lo: self.lo,
            hi: self.hi,
        }
    }
}

impl PartialEq for Cut {
    fn eq(&self, other: &Self) -> bool {
        same_capture(&self.capture, &other.capture) && self.lo == other.lo && self.hi == other.hi
    }
}

impl Eq for Cut {}

impl fmt::Debug for Cut {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Cut")
            .field("lo", &self.lo)
            .field("hi", &self.hi)
            .finish_non_exhaustive()
    }
}

impl Clone for Piece {
    fn clone(&self) -> Self {
        match self {
            Self::Literal(bytes) => Self::Literal(bytes.clone()),
            Self::Copy(cut) => Self::Copy(cut.clone()),
        }
    }
}

impl fmt::Debug for Piece {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Literal(bytes) => f.debug_tuple("Literal").field(bytes).finish(),
            Self::Copy(cut) => f.debug_tuple("Copy").field(cut).finish(),
        }
    }
}

impl Clone for Edit {
    fn clone(&self) -> Self {
        Self {
            target: self.target.clone(),
            parts: self.parts.clone(),
        }
    }
}

impl fmt::Debug for Edit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Edit")
            .field("target", &self.target)
            .field("parts", &self.parts)
            .finish()
    }
}

impl Clone for Refusal {
    fn clone(&self) -> Self {
        *self
    }
}

impl Copy for Refusal {}

impl PartialEq for Refusal {
    fn eq(&self, other: &Self) -> bool {
        matches!(
            (self, other),
            (Self::OutOfBounds, Self::OutOfBounds)
                | (Self::Overlap, Self::Overlap)
                | (Self::TooManySplices, Self::TooManySplices)
                | (Self::PayloadTooLarge, Self::PayloadTooLarge)
                | (Self::CandidateTooLarge, Self::CandidateTooLarge)
                | (Self::InvalidUtf8Boundary, Self::InvalidUtf8Boundary)
        )
    }
}

impl Eq for Refusal {}

impl fmt::Debug for Refusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::OutOfBounds => "OutOfBounds",
            Self::Overlap => "Overlap",
            Self::TooManySplices => "TooManySplices",
            Self::PayloadTooLarge => "PayloadTooLarge",
            Self::CandidateTooLarge => "CandidateTooLarge",
            Self::InvalidUtf8Boundary => "InvalidUtf8Boundary",
        })
    }
}
