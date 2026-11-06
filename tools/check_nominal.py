"""Produce and compile nominal API fixtures; failing cases must fail for the intended reason.

Exact invocation after building the domain crate:
    python3 tools/check_nominal.py --domain-rlib target/debug/libsplicery_domain.rlib --dependency-dir target/debug/deps --evidence /tmp/splicery-nominal
The evidence directory must be fresh and outside the source tree.
"""
from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import shlex
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))
from evidence_identity import source_identity

VALID_SIBLINGS = r"""
use splicery_domain::{CaptureBinding, EditTarget, OriginalImage, Part, ReadCapture, RecipeSpan, SourceSpan, Splice};

pub fn valid_siblings(image: &OriginalImage, lo: usize, hi: usize, start: usize, end: usize)
    -> Option<(SourceSpan, ReadCapture, CaptureBinding, Splice)>
{
    let source = SourceSpan::new(image, lo, hi)?;
    let capture = ReadCapture::new(image, lo, hi)?;
    let insertion = EditTarget::insertion(image, lo)?;
    let target = EditTarget::new(image, insertion.lo(), insertion.hi())?;
    let splice = Splice {
        target,
        replacement: vec![Part::Copy(source)],
        origin: RecipeSpan { start, end },
    };
    let binding = CaptureBinding { name: "capture".to_owned(), span: capture };
    Some((source, capture, binding, splice))
}
"""

# name: (source, rustc error code, required diagnostic fragments)
EXPECTED_FAILURES = {
    "source_span_private_field": (
        r"""
use splicery_domain::SourceSpan;
pub fn inspect(span: SourceSpan) -> usize { span.lo }
""",
        "E0616",
        ["private", "SourceSpan"],
    ),
    "read_capture_private_field": (
        r"""
use splicery_domain::ReadCapture;
pub fn inspect(capture: ReadCapture) -> usize { capture.lo }
""",
        "E0616",
        ["private", "ReadCapture"],
    ),
    "edit_target_private_field": (
        r"""
use splicery_domain::EditTarget;
pub fn inspect(target: EditTarget) -> usize { target.lo }
""",
        "E0616",
        ["private", "EditTarget"],
    ),
    "recipe_span_as_source_span": (
        r"""
use splicery_domain::{RecipeSpan, SourceSpan};
pub fn accept_source(_: SourceSpan) {}
pub fn misuse(origin: RecipeSpan) { accept_source(origin); }
""",
        "E0308",
        ["expected `SourceSpan`, found `RecipeSpan`"],
    ),
    "source_span_as_recipe_span": (
        r"""
use splicery_domain::{RecipeSpan, SourceSpan};
pub fn accept_origin(_: RecipeSpan) {}
pub fn misuse(span: SourceSpan) { accept_origin(span); }
""",
        "E0308",
        ["expected `RecipeSpan`, found `SourceSpan`"],
    ),
    "source_span_as_read_capture": (
        r"""
use splicery_domain::{CaptureBinding, SourceSpan};
pub fn forge(span: SourceSpan) -> CaptureBinding {
    CaptureBinding { name: "capture".to_owned(), span }
}
""",
        "E0308",
        ["expected `ReadCapture`, found `SourceSpan`"],
    ),
    "read_capture_as_edit_target": (
        r"""
use splicery_domain::{EditTarget, ReadCapture};
pub fn accept_target(_: EditTarget) {}
pub fn promote(capture: ReadCapture) { accept_target(capture); }
""",
        "E0308",
        ["expected `EditTarget`, found `ReadCapture`"],
    ),
    "edit_target_as_read_capture": (
        r"""
use splicery_domain::{EditTarget, ReadCapture};
pub fn accept_capture(_: ReadCapture) {}
pub fn demote(target: EditTarget) { accept_capture(target); }
""",
        "E0308",
        ["expected `ReadCapture`, found `EditTarget`"],
    ),
    "source_span_as_splice_target": (
        r"""
use splicery_domain::{RecipeSpan, SourceSpan, Splice};
pub fn forge(span: SourceSpan) -> Splice {
    Splice { target: span, replacement: Vec::new(), origin: RecipeSpan { start: 0, end: 0 } }
}
""",
        "E0308",
        ["expected `EditTarget`, found `SourceSpan`"],
    ),
    "read_capture_into_edit_target": (
        r"""
use splicery_domain::{EditTarget, ReadCapture};
pub fn promote(capture: ReadCapture) -> EditTarget { capture.into() }
""",
        "E0277",
        ["EditTarget: From<ReadCapture>"],
    ),
    "edit_target_new_recipe_span_argument": (
        r"""
use splicery_domain::{EditTarget, RecipeSpan};
pub fn forge(origin: RecipeSpan) -> Option<EditTarget> { EditTarget::new(origin, 0, 0) }
""",
        "E0308",
        ["expected `&OriginalImage`, found `RecipeSpan`"],
    ),
    "read_capture_new_source_span_argument": (
        r"""
use splicery_domain::{ReadCapture, SourceSpan};
pub fn forge(span: SourceSpan) -> Option<ReadCapture> { ReadCapture::new(span, 0, 0) }
""",
        "E0308",
        ["expected `&OriginalImage`, found `SourceSpan`"],
    ),
    "source_span_new_edit_target_argument": (
        r"""
use splicery_domain::{EditTarget, SourceSpan};
pub fn forge(target: EditTarget) -> Option<SourceSpan> { SourceSpan::new(target, 0, 0) }
""",
        "E0308",
        ["expected `&OriginalImage`, found `EditTarget`"],
    ),
}


def invoke(rustc: str, rlib: Path, dependencies: Path, output: Path,
           name: str, source: Path) -> dict:
    artifact = output / "artifacts" / f"{name}.rmeta"
    command = [
        rustc,
        "--edition=2024",
        "--crate-type=lib",
        "--crate-name", name,
        "--emit", f"metadata={artifact}",
        "--extern", f"splicery_domain={rlib}",
        "-L", f"dependency={dependencies}",
        "--error-format=human",
        "--color=never",
        str(source),
    ]
    command_line = shlex.join(command)
    print(command_line, flush=True)
    result = subprocess.run(command, cwd=output, capture_output=True, check=False, timeout=60)
    return {
        "command": command,
        "command_line": command_line,
        "exit_status": result.returncode,
        "stdout": result.stdout.decode("utf-8", "replace"),
        "stderr": result.stderr.decode("utf-8", "replace"),
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--domain-rlib", required=True, type=Path)
    parser.add_argument("--dependency-dir", type=Path,
                        help="directory containing domain dependencies (defaults to the rlib parent)")
    parser.add_argument("--evidence", required=True, type=Path)
    parser.add_argument("--rustc", default="rustc")
    args = parser.parse_args()
    rlib = args.domain_rlib.resolve()
    dependencies = (args.dependency_dir or rlib.parent).resolve()
    output = args.evidence.resolve()
    if not rlib.is_file():
        parser.error("domain rlib does not exist")
    if not dependencies.is_dir():
        parser.error("dependency directory does not exist")
    if output.is_relative_to(ROOT) or output.exists():
        parser.error("use a fresh evidence directory outside the source tree")
    output.mkdir(parents=True, mode=0o700)
    fixtures = output / "fixtures"
    artifacts = output / "artifacts"
    fixtures.mkdir()
    artifacts.mkdir()
    receipt = {
        "schema_version": 1,
        "gate": "SG04",
        "source_identity": source_identity(ROOT),
        "scope": "nominal read/edit capability and coordinate-unit APIs",
        "status": "failed",
        "domain_rlib": str(rlib),
        "cases": [],
    }

    def compile_case(name: str, source: str, expected_code: str | None,
                     fragments: list[str]) -> None:
        fixture = fixtures / f"{name}.rs"
        fixture.write_text(source, encoding="utf-8")
        observed = invoke(args.rustc, rlib, dependencies, output, name, fixture)
        case = {"name": name, "expected_error": expected_code, **observed}
        receipt["cases"].append(case)
        diagnostic = observed["stdout"] + observed["stderr"]
        if expected_code is None:
            if observed["exit_status"] != 0:
                raise ValueError(f"{name}: valid sibling did not compile")
        elif observed["exit_status"] == 0:
            raise ValueError(f"{name}: invalid use unexpectedly compiled")
        elif f"error[{expected_code}]" not in diagnostic or any(
                fragment not in diagnostic for fragment in fragments):
            raise ValueError(f"{name}: failed without expected {expected_code} diagnostic")

    try:
        compile_case("valid_siblings", VALID_SIBLINGS, None, [])
        for name, (source, code, fragments) in EXPECTED_FAILURES.items():
            compile_case(name, source, code, fragments)
        if receipt["source_identity"] != source_identity(ROOT):
            raise ValueError("checkout identity changed during nominal qualification")
        receipt["status"] = "passed_nominal_compile_fixtures"
    except (OSError, ValueError, subprocess.SubprocessError) as error:
        receipt["error"] = f"{type(error).__name__}: {error}"
    finally:
        descriptor = os.open(output / "nominal-receipt.json",
                             os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
        with os.fdopen(descriptor, "w", encoding="utf-8") as stream:
            json.dump(receipt, stream, indent=2, sort_keys=True)
            stream.write("\n")
    if receipt["status"] != "passed_nominal_compile_fixtures":
        print(receipt.get("error", "nominal checks failed"))
        return 1
    print(json.dumps({"status": receipt["status"], "cases": len(receipt["cases"])}, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
