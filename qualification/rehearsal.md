# Evidence for this handover

Run from the unpacked root using Python 3.10+ and the standard library:

    python3 -B tools/check_handover.py

This execution parses the supplied EBNF, recognizes complete recipe texts, checks expected syntax refusals, executes trusted example recipes through a development byte oracle and exhausts small one- and two-splice cases against an independent right-to-left application check. It explores a separate Python model of one logical write and checks that four deliberately weakened protocols have a counterexample. It verifies gate dependencies, relative Markdown links and, when present, every archive-member digest in MANIFEST.sha256. See [the captured output](authoring-result.json).

The Python parser/oracle handles the trusted distributed fixture set. It is not a hardened network-facing service and does not qualify syntax completeness, adversarial regex bounds, the host's actual grants, concurrency, or Rust. Do not publish it as an agent editing backend. The source-bound release suite must add fresh production and negative controls. The recipe vectors exist independently of the expected IR and code, so the implementation agent can build other backends without trusting Python's interpretation of Rust regex.

Lean 4.21.0 source and proof bodies are supplied under formal/. No Lean/Lake, Verus, TLC or TLAPS executable was available in the authoring environment. The TLA+ module, finite TLC configuration and TLAPS inductive seed are supplied for the workstation. These files have not received prover receipts here. Physical WSL file effects, Writfold integration and agent token benchmarks remain unexecuted. All SG00–SG14 product gate statuses remain not_run. First workstation step is the clean tool admission and proof run in AGENT_START_HERE.md.

The archive uses standard DEFLATE and one top-level directory, with UTF-8 text and ordinary relative member names. MANIFEST.sha256 covers all other files. External verification from inside the extracted directory: sha256sum -c MANIFEST.sha256. For the whole zip use unzip -t before extraction. No executable bit or installed package is required for the Python rehearsal.
