"""Generate an SPDX SBOM from the locked offline Cargo metadata closure."""
from __future__ import annotations

import argparse
from datetime import datetime, timezone
import hashlib
import json
from pathlib import Path
import subprocess
import tomllib

ROOT = Path(__file__).resolve().parents[1]


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--evidence", required=True, type=Path)
    args = parser.parse_args()
    output = args.evidence.resolve()
    if output.is_relative_to(ROOT) or output.exists():
        parser.error("use a fresh evidence directory outside the source tree")
    output.mkdir(parents=True, mode=0o700, exist_ok=False)
    command = [
        "docker", "run", "--rm", "--network", "none", "--workdir", "/work/splicery",
        "-v", f"{ROOT}:/work/splicery:ro",
        "-v", "splicery-rust-1.98.0-registry:/opt/cargo/registry",
        "splicery-tools:qualification", "cargo", "metadata", "--locked", "--offline",
        "--format-version", "1",
    ]
    result = subprocess.run(command, capture_output=True, check=False, timeout=300)
    (output / "cargo-metadata.json").write_bytes(result.stdout)
    if result.returncode:
        print(result.stderr.decode("utf-8", "replace"))
        return 1
    metadata = json.loads(result.stdout)
    lock = tomllib.loads((ROOT / "Cargo.lock").read_text())
    checksums = {(item["name"], item["version"]): item.get("checksum")
                 for item in lock["package"]}
    packages = []
    for package in sorted(metadata["packages"], key=lambda item: (item["name"], item["version"])):
        license_id = package.get("license") or "NOASSERTION"
        if license_id == "NOASSERTION":
            print(f"missing license for {package['name']}")
            return 1
        entry = {
            "SPDXID": f"SPDXRef-Package-{package['name']}-{package['version']}",
            "name": package["name"],
            "versionInfo": package["version"],
            "downloadLocation": package.get("source") or "NOASSERTION",
            "licenseConcluded": license_id,
            "licenseDeclared": license_id,
            "copyrightText": "NOASSERTION",
        }
        if package.get("source") and checksums[(package["name"], package["version"])]:
            entry["checksums"] = [{"algorithm": "SHA256",
                                   "checksumValue": checksums[(package["name"], package["version"])]}]
        packages.append(entry)
    document = {
        "spdxVersion": "SPDX-2.3",
        "dataLicense": "CC0-1.0",
        "SPDXID": "SPDXRef-DOCUMENT",
        "name": "splicery-locked-offline-workspace",
        "documentNamespace": f"https://writfold-dev.github.io/splicery/sbom/{hashlib.sha256(result.stdout).hexdigest()}",
        "creationInfo": {"created": datetime.now(timezone.utc).isoformat(), "creators": ["Tool: splicery-generate-sbom"]},
        "packages": packages,
    }
    (output / "sbom.spdx.json").write_text(json.dumps(document, indent=2, sort_keys=True) + "\n")
    print(json.dumps({"status": "generated", "packages": len(packages),
                      "sha256": hashlib.sha256((output / "sbom.spdx.json").read_bytes()).hexdigest()}, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
