# Splicery provenance and integration boundaries

## Input identity

- Input archive: `splicery-worktree-edit-handover-version-1.zip`
- SHA-256: `46855689f5f3042ab0790a7b8b98bef219bd1611f3059d6470a39d9bbf2c462f`
- Immutable imported baseline: commit `983c8c0` (`Import Splicery worktree edit handover`)
- Initial `python3 -B tools/check_handover.py`: passed 15 gate-graph checks, 9 grammar accepts, 13 grammar rejects, 11 recipe files, 9 extra refusals, 3 IR vectors, 4,092 one-operation and 18,675 two-operation byte cases, and four protocol counterexamples.
- `MANIFEST.sha256` and the archive remain unchanged. Later proof/tool edits intentionally drift from the imported baseline; current gate receipts hash their actual source closure instead of reusing the initial handover result.

## Sibling cross-check

The handover pins match both committed local HEADs exactly:

| Repository | Commit |
|---|---|
| `writfold-dev/writfold` | `b202ae28d41a883a21d1e3a082b5cb6426b385d6` |
| `writfold-dev/weft` | `db116fd773b593c8ee16cbb322f1822f3fb067a7` |

Both sibling worktrees also contain unrelated uncommitted operator work. Splicery reads them only as references and does not modify, commit, or claim evidence from that work. Weft proof reports carry no Splicery edit authority, and neither sibling is a Splicery kernel dependency.

## Admitted local tools and evidence

- Platform: Linux/WSL x86_64; container platform `linux/amd64`.
- Parent image: `writfold-tools@sha256:c854889f19c83174e69600ee28f0d4d35a0d2de1a23b3a35b1ecf4c4a88e0fc6`.
- Tool versions: Lean 4.21.0, Rust/Cargo 1.98.0, Verus 0.2026.09.06.8dea4a2, Z3 4.16.0, target `x86_64-unknown-linux-musl`.
- Storage is admitted only by the existing `ci-infra` observation and `~/.local/share/buildbot-ci/storage.json`; Splicery does not relocate Docker or caches.
- Qualification runs use Splicery-owned Compose caches, read-only source mounts, no network, 8 CPUs, 12 GiB memory and 1024 PIDs.
- Fresh evidence is private and outside the checkout under `~/workspace/splicery-evidence/`. Failed attempts are retained; repository commits contain source, not private raw evidence.

## Writfold/Weft boundaries

- Writfold P01O/WF24 retained-branch Omnigraph relocation remains blocked. Storage-dependent production integration must not adopt a fallback or claim a host receipt.
- The owner-published shared-stdio contract artifact required by WR5 is not yet published. Current uncommitted sibling stdio work is not a pinned integration contract.
- Weft WR6–WR8 procedure/run-start admissions remain external dependencies. A local Splicery or Weft binary cannot manufacture them.
- Revision 5 remains Writfold implementation authority; deferred revision-9 universal per-proposal Lean certificates are not assumed.
- Splicery preview and proof components do not enable production writes. The first write-enabled demonstration still requires the real Writfold host, Worktrunk binding, native materializer and applicable SG07–SG10/WF receipts.
