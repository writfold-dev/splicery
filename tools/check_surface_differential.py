"""Differential-check direct Lean surface meaning against a static Rust preview binary.

Exact invocation (with an already built static binary and a fresh output path):
    python3 -B tools/check_surface_differential.py \
      --rust-binary /path/to/splicery --lean-project formal \
      --evidence /tmp/splicery-surface-differential-fresh

The Lean producer is invoked as `lake env lean --run Splicery/Differential.lean`
inside --lean-project. No Python surface evaluator is used. The receipt compares
11 surface outcomes, 10 normalized preview reports, 14 individual normalized
primitive executions, and two explicit negative controls.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import struct
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))
from evidence_identity import source_identity
TARGETS = {
    "01-await": "sdk/src/client.rs",
    "02-wrap": "sdk/src/driver.rs",
    "03-policy": "docs/operations.md",
    "04-routes": "service/config/routes.conf",
    "05-swap": "docs/names.txt",
    "06-raw": "fixtures/frame.bin",
    "07-lines": "docs/config.ini",
    "08-indent": "docs/example.txt",
    "09-import": "sdk/src/lib.rs",
    "10-move": "cfg/service.conf",
    "11-absence": "data/table",
}
EXPECTED_MUTANTS = {
    "sequential-capture": ("05-swap", "6265746120626265746120"),
    "crlf-normalization": ("08-indent", "2020610a0a20206263"),
}


def invoke(command: list[str], cwd: Path, timeout: int = 600) -> dict:
    result = subprocess.run(command, cwd=cwd, capture_output=True, check=False, timeout=timeout)
    return {
        "command": command,
        "exit_status": result.returncode,
        "stdout_hex": result.stdout.hex(),
        "stderr": result.stderr.decode("utf-8", "replace"),
    }


def stdout(observed: dict) -> bytes:
    return bytes.fromhex(observed["stdout_hex"])


def input_bytes(vector: dict) -> bytes:
    if "input_text" in vector:
        return vector["input_text"].encode("utf-8")
    return bytes.fromhex(vector["input_hex"])


def output_bytes(vector: dict) -> bytes | None:
    if "refusal" in vector:
        return None
    if "output_text" in vector:
        return vector["output_text"].encode("utf-8")
    return bytes.fromhex(vector["output_hex"])


def static_elf(path: Path) -> dict:
    """Reject dynamic loaders and DT_NEEDED without executing host utilities."""
    data = path.read_bytes()
    if len(data) < 52 or data[:4] != b"\x7fELF":
        raise ValueError("Rust binary is not an ELF file")
    elf_class = data[4]
    encoding = data[5]
    if elf_class not in (1, 2) or encoding not in (1, 2):
        raise ValueError("unsupported ELF class or byte order")
    endian = "<" if encoding == 1 else ">"
    if elf_class == 1:
        phoff = struct.unpack_from(endian + "I", data, 28)[0]
        phentsize = struct.unpack_from(endian + "H", data, 42)[0]
        phnum = struct.unpack_from(endian + "H", data, 44)[0]
        minimum = 32
    else:
        phoff = struct.unpack_from(endian + "Q", data, 32)[0]
        phentsize = struct.unpack_from(endian + "H", data, 54)[0]
        phnum = struct.unpack_from(endian + "H", data, 56)[0]
        minimum = 56
    if phentsize < minimum or phoff + phentsize * phnum > len(data):
        raise ValueError("invalid ELF program header table")
    object_type = struct.unpack_from(endian + "H", data, 16)[0]
    dynamic_segments: list[tuple[int, int]] = []
    for index in range(phnum):
        at = phoff + index * phentsize
        segment_type = struct.unpack_from(endian + "I", data, at)[0]
        if segment_type == 3:  # PT_INTERP
            raise ValueError("Rust binary has a dynamic interpreter (PT_INTERP)")
        if segment_type == 2:  # PT_DYNAMIC
            if elf_class == 1:
                offset = struct.unpack_from(endian + "I", data, at + 4)[0]
                size = struct.unpack_from(endian + "I", data, at + 16)[0]
            else:
                offset = struct.unpack_from(endian + "Q", data, at + 8)[0]
                size = struct.unpack_from(endian + "Q", data, at + 32)[0]
            dynamic_segments.append((offset, size))
    needed = 0
    for offset, size in dynamic_segments:
        if offset + size > len(data):
            raise ValueError("invalid ELF dynamic segment")
        width = 8 if elf_class == 1 else 16
        for at in range(offset, offset + size, width):
            if at + width > offset + size:
                raise ValueError("truncated ELF dynamic entry")
            tag = struct.unpack_from(endian + ("I" if elf_class == 1 else "Q"), data, at)[0]
            if tag == 0:  # DT_NULL
                break
            if tag == 1:  # DT_NEEDED
                needed += 1
    if needed:
        raise ValueError("Rust binary has dynamic library dependencies (DT_NEEDED)")
    if not os.access(path, os.X_OK):
        raise ValueError("Rust binary is not executable")
    return {
        "format": "ELF32" if elf_class == 1 else "ELF64",
        "byte_order": "little" if encoding == 1 else "big",
        "object_type": object_type,
        "program_headers": phnum,
        "pt_interp": False,
        "dt_needed": 0,
    }


def normalized_projection(report: dict) -> list[dict]:
    if set(report) != {"schema_version", "scope", "path", "no_change", "candidate_length", "edits"}:
        raise ValueError("unexpected preview report fields")
    edits = []
    for edit in report["edits"]:
        if set(edit) != {"target", "replacement_hex", "origin"}:
            raise ValueError("unexpected normalized edit fields")
        edits.append({
            "target": edit["target"],
            "replacement_hex": edit["replacement_hex"],
            "origin": edit["origin"],
        })
    return edits


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--rust-binary", required=True, type=Path)
    parser.add_argument("--lean-project", required=True, type=Path)
    parser.add_argument("--evidence", required=True, type=Path)
    args = parser.parse_args()
    binary = args.rust_binary.resolve()
    project = args.lean_project.resolve()
    evidence = args.evidence.resolve()
    if evidence.is_relative_to(ROOT) or evidence.exists():
        parser.error("use a fresh evidence directory outside the source tree")
    evidence.mkdir(parents=True, mode=0o700, exist_ok=False)
    work = evidence / "work"
    work.mkdir(mode=0o700)
    receipt = {
        "schema_version": 1,
        "gate": "SG05",
        "source_identity": source_identity(ROOT),
        "scope": "direct Lean surface and normalized Rust preview differential",
        "status": "failed",
        "rust_binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
        "cases": [],
        "negative_mutants": [],
        "case_counts": {
            "surface_vectors": 0,
            "successful_surface_vectors": 0,
            "surface_refusals": 0,
            "normalized_preview_reports": 0,
            "normalized_primitive_executions": 0,
            "negative_mutants": 0,
        },
    }
    try:
        receipt["static_elf"] = static_elf(binary)
        vectors = json.loads((ROOT / "qualification/recipe-vectors.json").read_text(encoding="utf-8"))
        if [vector["name"] for vector in vectors] != list(TARGETS):
            raise ValueError("qualification vector set differs from the 11 surface targets")
        lean_command = ["lake", "env", "lean", "--run", "Splicery/Differential.lean"]
        lean_work = evidence / "lean-project"
        shutil.copytree(project, lean_work)
        build = invoke(["lake", "build"], lean_work)
        receipt["lean_build"] = build
        if build["exit_status"] != 0:
            raise ValueError("Lean project build failed")
        lean_run = invoke(lean_command, lean_work)
        receipt["lean_producer"] = lean_run
        if lean_run["exit_status"] != 0:
            raise ValueError("Lean differential producer failed; inspect lean_producer.stderr")
        direct = json.loads(stdout(lean_run))
        if (direct.get("schema_version") != 1
                or direct.get("scope") != "direct-lean-surface-differential"
                or direct.get("text_oracle") != "strict-rfc3629-utf8-codepoint-boundaries-v1"):
            raise ValueError("unexpected Lean differential receipt header")
        (evidence / "lean-direct-receipt.json").write_text(
            json.dumps(direct, indent=2, sort_keys=True) + "\n", encoding="utf-8"
        )
        lean_cases = direct.get("cases")
        if not isinstance(lean_cases, list) or [case.get("name") for case in lean_cases] != list(TARGETS):
            raise ValueError("Lean producer did not return exactly the 11 ordered vectors")
        primitive_count = 0
        report_count = 0
        for vector, lean in zip(vectors, lean_cases, strict=True):
            name = vector["name"]
            original = input_bytes(vector)
            expected = output_bytes(vector)
            if lean.get("input_hex") != original.hex():
                raise ValueError(f"{name}: Lean input bytes differ from qualification input")
            target = work / TARGETS[name]
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(original)
            recipe = work / f"{name}.spl"
            recipe.write_bytes((ROOT / "examples" / f"{name}.spl").read_bytes())
            command = [str(binary), "preview", "--root", str(work), str(recipe)]
            preview = invoke(command, work)
            case = {"name": name, "input_hex": original.hex(), "lean": lean, "preview": preview}
            receipt["cases"].append(case)
            receipt["case_counts"]["surface_vectors"] += 1
            if expected is None:
                if (lean.get("status") != "refusal"
                        or lean.get("refusal") != vector["refusal"]
                        or lean.get("direct_output_hex") is not None
                        or lean.get("primitive_output_hex") is not None
                        or lean.get("normalized_splices") is not None):
                    raise ValueError(f"{name}: Lean did not produce the exact absent-group refusal")
                if preview["exit_status"] != 2 or stdout(preview) != b"":
                    raise ValueError(f"{name}: Rust preview did not refuse")
                if vector["refusal"] not in preview["stderr"]:
                    raise ValueError(f"{name}: Rust refusal diagnostic is not {vector['refusal']}")
                if target.read_bytes() != original:
                    raise ValueError(f"{name}: refused preview modified its input")
                case["comparison"] = "exact_refusal_match"
                receipt["case_counts"]["surface_refusals"] += 1
                continue

            if lean.get("status") != "success":
                raise ValueError(f"{name}: Lean unexpectedly refused: {lean.get('refusal')}")
            direct_output = bytes.fromhex(lean["direct_output_hex"])
            primitive_output = bytes.fromhex(lean["primitive_output_hex"])
            if direct_output != expected or primitive_output != direct_output:
                raise ValueError(f"{name}: direct/lowered Lean output differs from the exact vector")
            if preview["exit_status"] != 0 or stdout(preview) != direct_output:
                raise ValueError(f"{name}: Rust preview bytes differ from direct Lean meaning")
            if target.read_bytes() != original:
                raise ValueError(f"{name}: preview modified its input")
            receipt["case_counts"]["successful_surface_vectors"] += 1

            report_run = invoke(command[:-1] + ["--report", command[-1]], work)
            case["preview_report"] = report_run
            if report_run["exit_status"] != 0:
                raise ValueError(f"{name}: Rust --report failed")
            report = json.loads(stdout(report_run))
            actual_normalized = normalized_projection(report)
            if actual_normalized != lean["normalized_splices"]:
                raise ValueError(f"{name}: normalized splices/origins differ from Lean")
            if (report["schema_version"] != 1 or report["scope"] != "offline-preview"
                    or report["path"] != TARGETS[name]
                    or report["candidate_length"] != len(direct_output)
                    or report["no_change"] != (direct_output == original)):
                raise ValueError(f"{name}: preview report metadata differs from exact after-image")
            case["normalized_splices"] = actual_normalized
            report_count += 1
            receipt["case_counts"]["normalized_preview_reports"] = report_count

            direct_changes = lean["direct_change_outputs_hex"]
            primitive_changes = lean["primitive_change_outputs_hex"]
            if (not isinstance(direct_changes, list) or direct_changes != primitive_changes
                    or len(primitive_changes) != len(actual_normalized)):
                raise ValueError(f"{name}: Lean per-change direct/primitive results differ")
            primitive_runs = []
            for splice, direct_change_hex in zip(actual_normalized, primitive_changes, strict=True):
                lo, hi = splice["target"]
                arguments = [
                    str(binary), "kernel", "apply", str(lo), str(hi - lo),
                    original.hex(), splice["replacement_hex"],
                ]
                observed = invoke(arguments, work)
                primitive_runs.append(observed)
                expected_stdout = direct_change_hex.encode("ascii") + b"\n"
                if observed["exit_status"] != 0 or stdout(observed) != expected_stdout:
                    raise ValueError(f"{name}: normalized Rust primitive differs from Lean lowering")
                primitive_count += 1
            case["normalized_primitive_executions"] = primitive_runs
            case["comparison"] = "exact_after_image_normalized_ir_origins_and_primitives_match"
            if target.read_bytes() != original:
                raise ValueError(f"{name}: report/primitive checks modified the input")
        receipt["case_counts"]["normalized_primitive_executions"] = primitive_count
        if primitive_count != 14 or report_count != 10:
            raise ValueError("wrong primitive/report case count")

        mutants = direct.get("mutants")
        if not isinstance(mutants, list) or {m.get("name") for m in mutants} != set(EXPECTED_MUTANTS):
            raise ValueError("Lean producer did not return both required negative controls")
        direct_by_name = {case["name"]: case for case in lean_cases}
        for mutant in mutants:
            name = mutant["name"]
            base, expected_mutant_hex = EXPECTED_MUTANTS[name]
            if (mutant.get("caught") is not True
                    or mutant.get("reference_hex") != direct_by_name[base]["direct_output_hex"]
                    or mutant.get("mutant_hex") != expected_mutant_hex
                    or mutant.get("mutant_hex") == mutant.get("reference_hex")):
                raise ValueError(f"negative control was not caught: {name}")
            receipt["negative_mutants"].append({**mutant, "base": base, "comparison": "caught"})
        receipt["case_counts"]["negative_mutants"] = len(mutants)
        receipt["case_counts"]["total_comparisons"] = (
            receipt["case_counts"]["surface_vectors"] + primitive_count + len(mutants)
        )
        if receipt["source_identity"] != source_identity(ROOT):
            raise ValueError("checkout identity changed during surface differential")
        receipt["status"] = "passed_direct_lean_surface_rust_differential"
    except (OSError, ValueError, KeyError, IndexError, TypeError, struct.error,
            json.JSONDecodeError, UnicodeError, subprocess.SubprocessError) as error:
        receipt["error"] = f"{type(error).__name__}: {error}"
    finally:
        descriptor = os.open(
            evidence / "surface-differential-receipt.json",
            os.O_WRONLY | os.O_CREAT | os.O_EXCL,
            0o600,
        )
        with os.fdopen(descriptor, "w", encoding="utf-8") as stream:
            json.dump(receipt, stream, indent=2, sort_keys=True)
            stream.write("\n")
    if receipt["status"] != "passed_direct_lean_surface_rust_differential":
        print(receipt.get("error", "surface differential failed"))
        return 1
    print(json.dumps({"status": receipt["status"], "case_counts": receipt["case_counts"]}, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
