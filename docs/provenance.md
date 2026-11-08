# Splicery provenance and integration boundaries

## Input identity

- Input archive: `splicery-worktree-edit-handover-version-1.zip`
- SHA-256: `46855689f5f3042ab0790a7b8b98bef219bd1611f3059d6470a39d9bbf2c462f`
- Immutable imported baseline: commit `983c8c0` (`Import Splicery worktree edit handover`)
- Initial `python3 -B tools/check_handover.py`: passed 15 gate-graph checks, 9 grammar accepts, 13 grammar rejects, 11 recipe files, 9 extra refusals, 3 IR vectors, 4,092 one-operation and 18,675 two-operation byte cases, and four protocol counterexamples.
- `MANIFEST.sha256` and the archive remain unchanged. Later proof/tool edits intentionally drift from the imported baseline; current gate receipts hash their actual source closure instead of reusing the initial handover result.

## Sibling cross-check

The original handover review used these exact pins:

| Repository | Commit |
|---|---|
| `writfold-dev/writfold` | `b202ae28d41a883a21d1e3a082b5cb6426b385d6` |
| `writfold-dev/weft` | `db116fd773b593c8ee16cbb322f1822f3fb067a7` |

At review start, the Writfold checkout was `8d411f60f8b4ca3fa2ed69e9c28c0a95848170f1`. The fresh receipt `~/.local/share/writfold/evidence/toolchain-smoke-splicery-resume-20261106-001.json` records eight passing component checks against Splicery `c696b6e3e60938c2b19081c38801e0d67ced8df2`, while full end-to-end status remained blocked. It binds executable hashes and exact source manifests, not the Writfold Git commit, and does not cover subsequent checkout changes. Splicery reads sibling sources only as references and does not modify them or add a mutable sibling dependency. Weft proof reports carry no Splicery edit authority.

## Admitted local tools and evidence

- Platform: Linux/WSL x86_64; container platform `linux/amd64`.
- Parent image: `writfold-tools@sha256:c854889f19c83174e69600ee28f0d4d35a0d2de1a23b3a35b1ecf4c4a88e0fc6`.
- Tool versions: Lean 4.21.0, Rust/Cargo 1.98.0, Verus 0.2026.09.06.8dea4a2, Z3 4.16.0, target `x86_64-unknown-linux-musl`.
- Storage is admitted only by the existing `ci-infra` observation and `~/.local/share/buildbot-ci/storage.json`; Splicery does not relocate Docker or caches.
- Qualification runs use Splicery-owned Compose caches, read-only source mounts, no network, 8 CPUs, 12 GiB memory and 1024 PIDs.
- Fresh evidence is private and outside the checkout under `~/workspace/splicery-evidence/`. Failed attempts are retained; repository commits contain source, not private raw evidence.

## Writfold/Weft boundaries

- Writfold P01O/WF24 retained-branch Omnigraph relocation remains blocked. Storage-dependent production integration must not adopt a fallback or claim a host receipt.
- Writfold's control-only stdio schemas, vectors and initialized session are now pinned and exercised. The owner-published worktree data/effect contract, persistent handoff and authorized execution methods required by SG08/WR5–WR8 are still absent; transport control alone does not close them.
- Weft WR6–WR8 procedure/run-start admissions remain external dependencies. A local Splicery or Weft binary cannot manufacture them.
- Revision 5 remains Writfold implementation authority; deferred revision-9 universal per-proposal Lean certificates are not assumed.
- Splicery preview and proof components do not enable production writes. The first write-enabled demonstration still requires the real Writfold host, Worktrunk binding, native materializer and applicable SG07–SG10/WF receipts.

## Offline packaged notices

`tools/package_licenses.py` implements the local archive/inventory convention without
importing a sibling checkout. It retains the unmodified project MIT license and
the actual LICENSE/LICENCE/COPYING/NOTICE/COPYRIGHT/AUTHORS texts (including
notice directories and Cargo `license_file`) from the locked offline Cargo
closure. The inventory distinguishes conservative target-runtime reachability
from build/test-only packages. Available originals from both are retained;
missing nonruntime originals remain explicit, not reported as complete coverage.
The inventory binds metadata, Cargo.lock, source/version/checksum, manifests,
notice paths/hashes, collector and archive. Schema 3 also records original runtime
distribution notices, target file hashes, whole-library source coverage and
optional source-bound final-link attribution. Cached `.crate` archives must match
Cargo.lock, and every packaged notice must match its original archive member.
Unsupported sources, missing runtime texts, escaping paths, symlinks and invalid
or oversized notice inputs refuse; SPDX IDs never substitute for original text.
Cargo texts remain limited to 1 MiB; original runtime
copyright HTML is bounded at 16 MiB. Source archive members are retained as
original bytes (including inline notices/data), not subjected to text rewriting.
The Verus Cargo packages omit root license files. Their original archive
`.cargo_vcs_info.json` binds commit
`53ec5c4b0770df8ba5526adb823929fb85eb0c4b`, package subtree and the original
upstream MIT license; this is distinct from the Verus compiler executable pin.
`convert_case 0.4.0` has no original notice text in its archive or original
repository root. It is a procedural-macro host dependency, not a target-runtime
dependency; its missing original remains recorded. No substitute MIT text is
invented and no redistribution claim is made for that build tool.
The ZIP uses stable paths/timestamps. Failed attempt files must be retained; retry
into a fresh evidence directory.

From this repository, with the existing admitted local image and warmed registry
volume, the parent can run the following offline collection. Resolve and retain
the immutable image ID rather than trusting a mutable tag across invocations:

```sh
export SPLICERY_TOOL_IMAGE=sha256:adcbd1fe3aa2692ad84f3bd7e61777b97cb8557c7d1b59775498af46a1a1e1f5
export SPLICERY_ATTEMPT=/srv/splicery-evidence/notices-NEW
mkdir -m 700 "$SPLICERY_ATTEMPT"
docker image inspect "$SPLICERY_TOOL_IMAGE" > "$SPLICERY_ATTEMPT/tool-image.json"
docker compose run --rm --no-deps qualify python3 -B tools/package_licenses.py --evidence /evidence/notices
```

Inputs are this read-only checkout (including LICENSE, Cargo.toml, Cargo.lock and
workspace manifests), the admitted tools image, and
`splicery-rust-1.98.0-registry` mounted at `/opt/cargo/registry`. The collector runs
inside that image so metadata package paths resolve to the exact cached sources.
No network fetch or host Cargo installation is required.
`tools/generate_sbom.py --evidence <fresh-outside-directory>` also collects these
texts under `notices/` and resolves the tools image once for both operations,
retaining `tool-image.json`. Its SPDX document describes Cargo packages only;
declared IDs are not legal conclusions or runtime notice evidence.

For a fresh build/extract attempt, stage the pinned sources below, select another
fresh `SPLICERY_ATTEMPT`, create it as above, then run:

```sh
docker compose run --rm --no-deps \
  -v "$SPLICERY_RUNTIME_SOURCE_DIR:/runtime-sources:ro" \
  -e SPLICERY_RUNTIME_SOURCES=/runtime-sources \
  qualify python3 -B scripts/run_build.py --rust-version 1.98.0 \
  --tool-image "$SPLICERY_TOOL_IMAGE" --evidence /evidence/build
docker build --network=none --pull=false -f docker/Dockerfile.runtime -t splicery:notice-review "$SPLICERY_ATTEMPT/build"
cid="$(docker create splicery:notice-review)"
docker cp "$cid:/usr/share/licenses/splicery" "$SPLICERY_ATTEMPT/extracted-notices"
docker cp "$cid:/usr/bin/splicery" "$SPLICERY_ATTEMPT/extracted-splicery"
docker rm "$cid"
```

The build additionally uses `splicery-rust-1.98.0-target`. It first builds the
**complete workspace**, then explicitly relinks `--bin splicery-cli` with a
fresh final-link map, copies that resulting executable, and collects notices
against its map/receipt/source identity. The original workspace artifact snapshot
and `final_link_artifacts` are separate observations; `client` is the latter
executable. Existing static-ELF/smoke checks exercise that final executable.
Notice artifact and link-receipt hashes are recorded in `build-results.json`.
The map path is unique per attempt even when containers mount different host
directories at the same `/evidence` pathname. Otherwise Cargo can reuse a cached
link and leave a new attempt without a map.
The scratch image requires `notices/LICENSE`, `notices/third-party-licenses.zip`
and `notices/third-party-licenses.json`; an executable-only context fails.
The parent must compare extracted bytes/hashes with the build inventory.

### Observed runtime inputs and remaining boundary

Read-only inspection of the immutable image above found:

- `rustc 1.98.0`, commit `88d9e12ae178fab0fb5cc050a94da85685d449ea`,
  LLVM 22.1.8. The executable is checked against `toolchains.lock.json`.
- The installed channel manifest is dated `2026-08-20` and has SHA-256
  `b17aaaeded2ac77b968b406613ec319b001f105793900a6f852a72a03e241ba9`.
  It pins `https://static.rust-lang.org/dist/2026-08-20/rust-std-1.98.0-x86_64-unknown-linux-musl.tar.xz`
  to `1a76f782db2d540e1cd16ea47829b323f4a8f4dda64bca4b23be189109c510f8`.
  This is recorded distribution provenance, not a claim that a newly downloaded
  archive was compared with every installed object.
- `share/doc/rust/COPYRIGHT-library.html` (1,512,520 bytes),
  `COPYRIGHT.html` (15,380,909 bytes) and all twelve accompanying license texts
  are present and retained verbatim, not reconstructed from SPDX labels.
  The whole-distribution copyright file is a conservative superset; its presence
  does not imply every described component is linked or every external C runtime
  notice is included. Installed `rustc-src` contains compiler/proc_macro source,
  not the complete standard-library, musl or LLVM source trees.
- Rust's target manifest includes `libcompiler_builtins`, `libunwind`, and
  `self-contained/{libc.a,libunwind.a,crt*.o,Scrt1.o,rcrt1.o}`.
  The collector hashes every installed target-manifest file, but does **not**
  label those files as selected linker inputs.
- The pinned [Rust musl build recipe](https://github.com/rust-lang/rust/blob/88d9e12ae178fab0fb5cc050a94da85685d449ea/src/ci/docker/scripts/musl-toolchain.sh)
  selects musl 1.2.5 plus two retained CVE patches and musl-cross-make commit
  `3635262e4524c991552789af6f36211a335a77b3`. That commit's
  [Makefile](https://github.com/richfelker/musl-cross-make/blob/3635262e4524c991552789af6f36211a335a77b3/Makefile)
  selects GCC **9.4.0**, despite the older version in the recipe's introductory
  comment. Read-only `readelf -p .comment` inspection of bundled `crtbegin*.o`
  and `crtend*.o` also reports GCC 9.4.0.
- The image's distro musl is **1.2.4-2**, and GCC/libgcc development packages are
  **13.3.0-6ubuntu2~24.04.1**. Their original Debian copyright records and
  `musl-gcc` wrapper/specs are retained separately. They must not be substituted
  for Rust's bundled musl 1.2.5/GCC 9.4.0 notices.

### Pinned supplemental notices

The collector is offline. `RUNTIME_SOURCES` in `tools/package_licenses.py` gives
the exact paths, immutable commit URLs and SHA-256 hashes of fifteen original
notice/provenance files: the Verus Cargo-source MIT license, compiler-builtins
and its libm notices, Rust's library lock/build recipe/patches, LLVM libunwind and
compiler-rt licenses/credits, GCC 9.4.0 GPL/runtime exception and original
`crtstuff.c`, musl 1.2.5 COPYRIGHT, and the pinned cross-make Makefile. LLVM revision
`52ed14fcd56afc30f9cccd8ca8ce237c2eef7e04` comes from the
[Rust commit's submodule entry](https://github.com/rust-lang/rust/tree/88d9e12ae178fab0fb5cc050a94da85685d449ea/src/llvm-project);
GCC revision `13c83c4cc679ad5383ed57f359e53e8d518b7842` is the
peeled 9.4.0 release tag. No latest source is accepted.

`RUNTIME_ARCHIVES` additionally pins whole-source coverage:

| Local input | Original source | SHA-256 |
|---|---|---|
| `rust-src.tar.xz` | Rust 1.98.0 dated 2026-08-20 distribution | `0e977492aed5ff137815bf9b4b796e8a85b80fb2367ed64e82d5c5839870f2bf` |
| `musl-1.2.5.tar.gz` | musl 1.2.5 release | `a9a118bbe84d8764da0ea0d28b3ab3fae8477fc7e4085d90102b8596fc7c75e4` |
| `llvm-project.tar.gz` | LLVM commit above | `27f31a6639023d90cd754c1de42d8f9d77cc0c3deff37896f88c1346c6c3f396` |
| `musl-cross-make.tar.gz` | musl-cross-make commit above | `60bed670d689d5c2164020960df36b80189dc5617e9763e023672f98a99d567e` |

Exact URLs live alongside those hashes in the collector. All Rust source
distribution, musl and cross-make regular files are retained; from LLVM only the
complete `libunwind/` and `compiler-rt/lib/builtins/` subtrees are retained.
The full LLVM acquisition is approximately 258 MB, but its retained source
subtrees are approximately 1.9 MB. Cross-make contains the two earlier musl
CVE-2025-26519 patches as well as GCC patch provenance; Rust's two later musl
patches remain separate original diffs. No patched source is passed off as an
unmodified upstream release. Each archive and retained member has its own hash.
No archive is extracted onto the host filesystem by the collector.

The acquired fifteen texts and four archives are staged at
`/srv/splicery-evidence/runtime-source-coverage-k3286whh/`; the parent may
use that directory read-only instead of downloading again.

The parent can stage these exact inputs once in admitted operator storage.
This acquisition is separate from offline qualification, does not compile
anything, and retains failed download bytes rather than overwriting an attempt:

```sh
export SPLICERY_RUNTIME_SOURCE_DIR=/srv/splicery-evidence/runtime-sources-NEW
python3 -B - "$SPLICERY_RUNTIME_SOURCE_DIR" <<'PY'
import hashlib
from pathlib import Path
import sys
from urllib.request import urlopen
from tools.package_licenses import RUNTIME_SOURCES, RUNTIME_ARCHIVES
root = Path(sys.argv[1])
root.mkdir(mode=0o700, parents=True, exist_ok=False)
inputs = {**RUNTIME_SOURCES,
          **{name: spec[:2] for name, spec in RUNTIME_ARCHIVES.items()}}
for relative, (url, expected) in inputs.items():
    path = root / relative
    path.parent.mkdir(parents=True, exist_ok=True)
    digest = hashlib.sha256()
    with path.open("xb") as stream, urlopen(url, timeout=300) as response:
        while chunk := response.read(1024 * 1024):
            stream.write(chunk)
            digest.update(chunk)
    if digest.hexdigest() != expected:
        raise SystemExit(f"checksum differs; retained {path}")
PY
```

Choose a fresh `SPLICERY_ATTEMPT`, create it and retain the image inspection as
above. Collect with the staged directory mounted read-only:

```sh
docker compose run --rm --no-deps \
  -v "$SPLICERY_RUNTIME_SOURCE_DIR:/runtime-sources:ro" \
  qualify python3 -B tools/package_licenses.py \
  --runtime-sources /runtime-sources --evidence /evidence/notices
```

The earlier build command passes this same mount/environment to its post-link
collector. For the host SBOM command, pass `--tool-image "$SPLICERY_TOOL_IMAGE"
--runtime-sources "$SPLICERY_RUNTIME_SOURCE_DIR"`. That standalone Cargo SBOM
does not build or supply a final-link receipt and therefore does not claim
observed-binary coverage. Missing optional staging is explicit in
`missing_supplements`/`missing_source_archives`; a supplied incomplete or altered
directory fails collection. Notices, source coverage and link provenance are
embedded under `runtime/` in the same ZIP required by `Dockerfile.runtime`; no
additional Dockerfile copy or executable behavior is needed.

### Observed-link source-family attribution

The parent subsequently supplied
`/srv/splicery-evidence/runtime-notice-closure-7svxnt_1/link-003.json`.
Its original map hash is
`6d0f1b92ebc07ef5e9bef95510b2749b76b2c7107675499040eba30c336653ec`
and executable hash is
`db79d50147cc51d7183b18474d1ea9f3f6eb935e14f7c7f8719e7fdea3965081`.
Read-only review identified 55 `LOAD` records, five LLVM libunwind archive-member
records and 115 musl libc archive-member records. The map selects Rust's bundled
`rcrt1.o`, `crti.o`, `crtbeginS.o`, `crtendS.o`, `crtn.o`, `libc.a`,
`libunwind.a`, compiler-builtins rlib, and Rust/Cargo-generated LTO objects.
It does not select the distro's musl/GCC runtime archives.
This observation informed the source-family mapping; it is **not** reused as
the receipt of a later packaged executable.

The correct isolated inspection command includes the explicit binary target:

```sh
cargo rustc --release --locked --offline -p splicery-cli --bin splicery-cli \
  --target x86_64-unknown-linux-musl -- \
  -C link-arg=-Wl,-Map,/evidence/splicery-link.map -C link-arg=-Wl,-t
```

`run_build.py` now performs that final-link observation after, not instead of,
its full-workspace build, with JSON Cargo artifact output and a fresh map path.
`link-evidence.json` binds the exact final executable, map, command, logs,
parent-supplied immutable image identity and unchanged source manifest. The
collector checks those bytes, the map's actual output binary, all manifest
source hashes and the observed input paths. Unexpected native/distro inputs
or unrecognized LTO/Cargo families are refused rather than attributed by guess.
The source identity includes the packaging implementation and docs: old receipts
must not be used to label a changed source tree.

Attribution is deliberately conservative:

- Rust std/alloc/core and compiler-builtins retain complete original `rust-src`,
  the distribution copyright/dependency notice documents and license texts,
  plus compiler-builtins/libm original notices. LTO inputs are mapped to this
  whole-library coverage, never invented source-line precision.
- musl 1.2.5 retains its entire original source tree, including every inline
  third-party notice, COPYRIGHT and original patch provenance. This includes
  the selected libc members without relying solely on top-level MIT text.
  COPYRIGHT's header/CRT attribution exception is preserved, not generalized
  to libc or other third-party source files.
- LLVM libunwind retains its complete original source subtree and original
  LICENSE; compiler-rt builtins retain their complete original subtree and
  original license/credits. At the pinned revision `libunwind/CREDITS.TXT`
  does not exist, and the parent's path-history lookup returned no commits.
  The original libunwind LICENSE itself contains the copyright notice,
  conditions and disclaimer it requires reproduced. It is preserved verbatim;
  no separate nonexistent file is invented and compiler-rt credits are not
  substituted for it.
- GCC 9.4.0 CRT retains original `crtstuff.c`, GPL-3 and Runtime Library
  Exception 3.1, with original cross-make patch provenance. Recording these
  originals does not assert a legal determination of exception eligibility.
- Cargo and workspace-generated objects retain target-runtime package notices
  and the source-bound project MIT license. Build/test-only missing originals
  remain separate inventory entries, never runtime coverage.

Only when the pinned source coverage **and** a successful matching link receipt
are present does `runtime_notice_closure.status` become
`source_covered_observed_link`. Without either it remains `unresolved` and names
the missing inputs. This is a bounded source/notice coverage result for the
hashed observed executable, not blanket legal clearance or release admission.

### Exercised packaging component

Under `/srv/splicery-evidence/runtime-notice-closure-7svxnt_1/`:

- `build-notices-005.json` and `build-notices-006.json` passed full-workspace
  builds, distinct final-link observations, notice collection and executable
  kernel smoke. Both produced executable
  `db79d50147cc51d7183b18474d1ea9f3f6eb935e14f7c7f8719e7fdea3965081`.
  The inventory records 43 Cargo packages, 28 conservative runtime dependencies,
  55 observed link inputs and the explicit nonruntime `convert_case` omission.
- `scratch-package-001.json` passed actual scratch-image construction, exact
  extracted executable/license/ZIP/inventory comparisons, kernel execution and
  read-only preview of the website fixture. Candidate bytes matched exactly;
  the input remained unchanged.
- `refusal-controls-001.json` passed three actual collector controls:
  corrupted original notice checksum, source-manifest drift and corrupted
  final-link map. None emitted a successful inventory/archive.
- `sbom-001.json` exercised the standalone command with 43 packages. It
  correctly retained `unresolved` runtime closure without a final-link receipt.

Failed attempts 001–004 remain retained: ordinary Cargo cache lacks vendored
`.cargo-checksum.json`; a reused container pathname did not force a fresh link;
`convert_case` lacks original notices; and Verus crates need their actual
upstream source license. No failed attempt was relabelled.

These receipts bind their exact source manifests, not subsequent documentation
edits or a clean release commit. This is observed-binary source/notice coverage,
not SG13 completion, general legal clearance, public-ready status, or permission
to delete the historical Weft asset.
