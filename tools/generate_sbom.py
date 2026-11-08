"""Generate an SPDX SBOM from the locked offline Cargo metadata closure."""
from __future__ import annotations

import argparse
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tomllib

ROOT = Path(__file__).resolve().parents[1]


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--evidence", required=True, type=Path)
    parser.add_argument("--tool-image", default=os.environ.get("SPLICERY_TOOL_IMAGE", "splicery-tools:qualification"))
    parser.add_argument("--runtime-sources", type=Path,
                        help="offline pinned runtime source directory; see docs/provenance.md")
    args = parser.parse_args()
    output = args.evidence.resolve()
    if output.is_relative_to(ROOT) or output.exists():
        parser.error("use a fresh evidence directory outside the source tree")
    output.mkdir(parents=True, mode=0o700, exist_ok=False)
    inspected = subprocess.run(["docker", "image", "inspect", args.tool_image],
                               capture_output=True, check=False, timeout=30)
    (output / "tool-image.json").write_bytes(inspected.stdout)
    (output / "tool-image.stderr").write_bytes(inspected.stderr)
    if inspected.returncode:
        print("Cannot resolve the admitted local tools image; inspect retained image logs.")
        return 1
    image = json.loads(inspected.stdout)[0]["Id"]
    if not image.startswith("sha256:"):
        raise ValueError("tools image has no immutable local identity")
    command = [
        "docker", "run", "--rm", "--network", "none", "--workdir", "/work/splicery",
        "-v", f"{ROOT}:/work/splicery:ro",
        "-v", "splicery-rust-1.98.0-registry:/opt/cargo/registry:ro",
        image, "cargo", "metadata", "--locked", "--offline",
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
            "licenseConcluded": "NOASSERTION",
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
        "comment": "Cargo metadata scope only. Original runtime notice evidence and unresolved final-link attribution are recorded in notices/third-party-licenses.json; no complete binary SBOM or redistribution closure is asserted.",
        "packages": packages,
    }
    (output / "sbom.spdx.json").write_text(json.dumps(document, indent=2, sort_keys=True) + "\n")
    notice_command = [
        "docker", "run", "--rm", "--network", "none", "--workdir", "/work/splicery",
        "-v", f"{ROOT}:/work/splicery:ro",
        "-v", f"{output}:/evidence",
        "-v", "splicery-rust-1.98.0-registry:/opt/cargo/registry:ro",
    ]
    if args.runtime_sources is not None:
        sources = args.runtime_sources.absolute()
        if not sources.is_dir() or sources.is_symlink():
            raise ValueError("runtime sources must be an existing non-symlink directory")
        notice_command.extend(["-v", f"{sources}:/runtime-sources:ro"])
    notice_command.extend([
        image, "python3", "-B", "tools/package_licenses.py",
        "--evidence", "/evidence/notices",
    ])
    if args.runtime_sources is not None:
        notice_command.extend(["--runtime-sources", "/runtime-sources"])
    notices = subprocess.run(notice_command, capture_output=True, check=False, timeout=300)
    (output / "package-licenses.stdout").write_bytes(notices.stdout)
    (output / "package-licenses.stderr").write_bytes(notices.stderr)
    if notices.returncode:
        print("Notice collection failed; inspect retained package-licenses logs.")
        return 1
    print(json.dumps({"status": "generated", "packages": len(packages),
                      "sha256": hashlib.sha256((output / "sbom.spdx.json").read_bytes()).hexdigest()}, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
