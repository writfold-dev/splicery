"""Exercise every shipped /1 recipe and the live-to-frozen offline workflow."""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[1]
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
PATTERN = r"^route[ \t]+(?<verb>[A-Z]+)[ \t]+(?<path>/\S+)$"


def run(binary: Path, work: Path, arguments: list[str]) -> dict:
    result = subprocess.run([str(binary), *arguments], cwd=work, capture_output=True, check=False, timeout=120)
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
    work.mkdir(parents=True)
    vectors = json.loads((ROOT / "qualification/recipe-vectors.json").read_text())
    receipt = {"schema_version": 1, "scope": "complete offline /1 language component",
               "status": "failed", "cases": []}

    def case(name: str, arguments: list[str], status: int, expected: bytes | None) -> dict:
        observed = run(binary, work, arguments)
        receipt["cases"].append({"name": name, "arguments": arguments, **observed})
        if observed["exit_status"] != status:
            raise ValueError(f"{name}: exit {observed['exit_status']} != {status}: {observed['stderr']}")
        if expected is not None and observed["stdout_hex"] != expected.hex():
            raise ValueError(f"{name}: bytes differ: {observed['stdout_hex']} != {expected.hex()}")
        return observed

    try:
        for vector in vectors:
            name = vector["name"]
            target = work / TARGETS[name]
            target.parent.mkdir(parents=True, exist_ok=True)
            original = vector.get("input_text", "").encode() if "input_text" in vector else bytes.fromhex(vector["input_hex"])
            target.write_bytes(original)
            recipe = work / f"{name}.spl"
            recipe.write_bytes((ROOT / "examples" / f"{name}.spl").read_bytes())
            if "refusal" in vector:
                case(name, ["preview", "--root", str(work), str(recipe)], 2, None)
                assert target.read_bytes() == original
                continue
            expected = vector.get("output_text", "").encode() if "output_text" in vector else bytes.fromhex(vector["output_hex"])
            case(name, ["preview", "--root", str(work), str(recipe)], 0, expected)
            if target.read_bytes() != original:
                raise ValueError(f"{name}: preview modified the target")

        frozen_json = work / "routes-frozen.json"
        inspected = case("inspect_routes", [
            "inspect", "--root", str(work), "--file", TARGETS["04-routes"],
            "--name", "@hits", "--expect", "3", PATTERN,
        ], 0, None)
        frozen_document = json.loads(bytes.fromhex(inspected["stdout_hex"]))
        original_routes = (work / TARGETS["04-routes"]).read_bytes()
        if (frozen_document["schema_version"] != 1 or frozen_document["name"] != "hits"
                or frozen_document["capture_sha256"] != hashlib.sha256(original_routes).hexdigest()
                or len(frozen_document["members"]) != 3):
            raise ValueError("inspect produced invalid frozen evidence")
        frozen_json.write_text(json.dumps(frozen_document))
        replay = work / "routes-replay.spl"
        replay.write_text('edit "service/config/routes.conf" {\n'
                          '  each @r in frozen @hits {\n'
                          '    replace @r with [keep(@r.verb), " ", keep(@r.path)]\n'
                          '  }\n'
                          '}\n')
        expected_routes = next(vector["output_text"] for vector in vectors if vector["name"] == "04-routes").encode()
        case("frozen_replay", ["preview", "--root", str(work), "--frozen", f"@hits={frozen_json}", str(replay)],
             0, expected_routes)

        create = work / "create.spl"
        create.write_text('create "api/README.md" with "hello\\n"')
        created = case("create_plan", ["preview", "--root", str(work), str(create)], 0, None)
        if json.loads(bytes.fromhex(created["stdout_hex"]))["content_hex"] != b"hello\n".hex():
            raise ValueError("create plan content differs")
        if (work / "api/README.md").exists():
            raise ValueError("create plan performed an effect")

        rename = work / "rename.spl"
        rename.write_text('rename "api/old" to "api/new"')
        renamed = case("rename_plan", ["preview", "--root", str(work), str(rename)], 0, None)
        if json.loads(bytes.fromhex(renamed["stdout_hex"]))["op"] != "rename":
            raise ValueError("rename plan has wrong operation")

        remove = work / "remove.spl"
        remove.write_text('remove "api/a.txt"')
        removed = case("remove_plan", ["preview", "--root", str(work), str(remove)], 0, None)
        if json.loads(bytes.fromhex(removed["stdout_hex"]))["op"] != "remove":
            raise ValueError("remove plan has wrong operation")

        raw_create = work / "raw-create.spl"
        raw_create.write_text('create "fixtures/new.bin" as bytes with hex"00ff"')
        raw = case("raw_create_plan", ["preview", "--root", str(work), str(raw_create)], 0, None)
        if json.loads(bytes.fromhex(raw["stdout_hex"]))["content_hex"] != "00ff":
            raise ValueError("raw create content differs")

        bad_create = work / "bad-create.spl"
        bad_create.write_text('create "api/bad" with keep(@x)')
        case("create_keep_refusal", ["preview", "--root", str(work), str(bad_create)], 2, None)

        bad_regex = work / "bad-regex.spl"
        bad_regex.write_text('edit "service/config/routes.conf" { each @r in line_matches rx"(?i)route" expect 1 { delete @r } }')
        case("inline_flag_refusal", ["preview", "--root", str(work), str(bad_regex)], 2, None)
        receipt["status"] = "passed_complete_offline_language_component"
    except (OSError, ValueError, KeyError, IndexError, json.JSONDecodeError) as error:
        receipt["error"] = f"{type(error).__name__}: {error}"
    finally:
        descriptor = os.open(evidence / "language-receipt.json", os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
        with os.fdopen(descriptor, "w", encoding="utf-8") as stream:
            json.dump(receipt, stream, indent=2, sort_keys=True)
            stream.write("\n")
    if receipt["status"] != "passed_complete_offline_language_component":
        print(receipt.get("error", "language checks failed"))
        return 1
    print(json.dumps({"status": receipt["status"], "cases": len(receipt["cases"])}, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
