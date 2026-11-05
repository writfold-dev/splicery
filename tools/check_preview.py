"""Exercise the actual Splicery preview binary on owned temporary files."""
from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import subprocess


def run(binary: Path, root: Path, arguments: list[str]) -> dict:
    result = subprocess.run([str(binary), *arguments], cwd=root, capture_output=True, check=False, timeout=120)
    return {"exit_status": result.returncode, "stdout_hex": result.stdout.hex(),
            "stderr": result.stderr.decode("utf-8", "replace")}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", required=True, type=Path)
    parser.add_argument("--evidence", required=True, type=Path)
    args = parser.parse_args()
    binary = args.binary.resolve()
    evidence = args.evidence.resolve()
    if evidence.exists():
        parser.error("evidence directory must be fresh")
    work = evidence / "work"
    target = work / "sdk/src/client.rs"
    target.parent.mkdir(parents=True)
    original = b"fn demo() {\n    transport.send(request)\n}\n"
    target.write_bytes(original)
    needle = b"transport.send(request)"
    start = original.index(needle)
    receipt = {"schema_version": 1, "scope": "offline preview end-to-end component",
               "status": "failed", "cases": []}

    def case(name: str, arguments: list[str], status: int, expected: bytes | None) -> None:
        observed = run(binary, work, arguments)
        receipt["cases"].append({"name": name, "arguments": arguments, **observed})
        if observed["exit_status"] != status:
            raise ValueError(f"{name}: exit {observed['exit_status']} != {status}: {observed['stderr']}")
        if expected is not None and observed["stdout_hex"] != expected.hex():
            raise ValueError(f"{name}: candidate bytes differ")
        if status != 0 and not observed["stderr"]:
            raise ValueError(f"{name}: refusal had no diagnostic")

    try:
        wrap = work / "wrap.spl"
        wrap.write_text('edit "sdk/src/client.rs" {\n'
                        '  @call := one text "transport.send(request)"\n'
                        '  replace @call with ["trace_send(", keep(@call), ")"]\n'
                        '}\n')
        expected = original.replace(needle, b"trace_send(" + needle + b")")
        case("captured_wrap", ["preview", "--root", str(work), str(wrap)], 0, expected)
        case("captured_wrap_report", ["preview", "--root", str(work), "--report", str(wrap)], 0, None)
        report = json.loads(bytes.fromhex(receipt["cases"][-1]["stdout_hex"]))
        if (report["scope"] != "offline-preview" or report["no_change"] is not False
                or report["edits"][0]["target"] != [start, start + len(needle)]
                or report["edits"][0]["replacement_hex"] != (b"trace_send(" + needle + b")").hex()):
            raise ValueError("exact splice report disagrees with the edit")

        multiline = work / "multiline.spl"
        multiline.write_text('replace @call with ["trace_send(", keep(@call), ")"]\n')
        capture = f"@call={start}:{start + len(needle)}"
        case("interactive_capture", ["preview", "--root", str(work), "--file", "sdk/src/client.rs",
                                    "--capture", capture, str(multiline)], 0, expected)

        insertion = work / "insert.spl"
        insertion.write_text('@call := one text "transport.send(request)"\n'
                             'before @call insert "A"\nafter @call insert "B"\n')
        case("before_after", ["preview", "--root", str(work), "--file", "sdk/src/client.rs",
                              str(insertion)], 0,
             original[:start] + b"A" + needle + b"B" + original[start + len(needle):])

        deletion = work / "delete.spl"
        deletion.write_text("delete @call\n")
        case("delete", ["preview", "--root", str(work), "--file", "sdk/src/client.rs",
                        "--capture", capture, str(deletion)], 0, original.replace(needle, b""))

        unchanged = work / "unchanged.spl"
        unchanged.write_text("replace @call with keep(@call)\n")
        case("no_change", ["preview", "--root", str(work), "--file", "sdk/src/client.rs",
                           "--capture", capture, str(unchanged)], 0, original)
        case("no_change_report", ["preview", "--root", str(work), "--file", "sdk/src/client.rs",
                                  "--capture", capture, "--report", str(unchanged)], 0, None)
        no_change_report = json.loads(bytes.fromhex(receipt["cases"][-1]["stdout_hex"]))
        if no_change_report["no_change"] is not True:
            raise ValueError("no-change report did not preserve the explicit result")

        ambiguous = work / "ambiguous.spl"
        ambiguous.write_text('delete one text "e"\n')
        case("ambiguous", ["preview", "--root", str(work), "--file", "sdk/src/client.rs",
                           str(ambiguous)], 2, None)

        overlap = work / "overlap.spl"
        overlap.write_text("replace range [0,2) with \"X\"\nreplace range [1,3) with \"Y\"\n")
        case("overlap", ["preview", "--root", str(work), "--file", "sdk/src/client.rs",
                         str(overlap)], 2, None)

        unicode_file = work / "unicode.txt"
        unicode_file.write_bytes("é".encode())
        boundary = work / "boundary.spl"
        boundary.write_text('replace range [1,2) with "x"\n')
        case("utf8_boundary", ["preview", "--root", str(work), "--file", "unicode.txt",
                               str(boundary)], 2, None)

        raw = work / "raw.spl"
        raw.write_text('edit "unicode.txt" as bytes { replace range [1,2) with hex"78" }\n')
        case("raw_byte", ["preview", "--root", str(work), str(raw)], 0, b"\xc3x")

        case("path_escape", ["preview", "--root", str(work), "--file", "../outside",
                             str(deletion)], 2, None)
        receipt["status"] = "passed_offline_preview_component"
    except (OSError, ValueError, KeyError, IndexError, json.JSONDecodeError) as error:
        receipt["error"] = f"{type(error).__name__}: {error}"
    finally:
        descriptor = os.open(evidence / "preview-receipt.json", os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
        with os.fdopen(descriptor, "w", encoding="utf-8") as stream:
            json.dump(receipt, stream, indent=2, sort_keys=True)
            stream.write("\n")
    if receipt["status"] != "passed_offline_preview_component":
        print(receipt.get("error", "preview checks failed"))
        return 1
    print(json.dumps({"status": receipt["status"], "cases": len(receipt["cases"])}, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
