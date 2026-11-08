# Splicery · worktree edit protocol /1

**Implementation handover 1 · 5 November 2026 · WSL/Linux x86_64.** Start in [AGENT_START_HERE.md](AGENT_START_HERE.md). This is a new, narrow Writfold-family workstream. The existing Splicery 0.6 explorations retain their own scope and version history. This package supplies a language contract, proof source, executable rehearsal and acceptance plan. Production capabilities require the gates in this package and the applicable owner-side Writfold gates.

An agent selects an exact piece of source and writes what it wants to do with it. The kernel copies retained bytes, so preserving a long expression costs the agent a short handle.

~~~splicery
edit "sdk/src/client.rs" {
  @call := one text "transport.send(request)"
  replace @call with ["trace_send(", keep(@call), ")"]
}
~~~

**Choose your starting point**

| If you want to… | Open |
|---|---|
| Implement the first slice | [Agent intake and first PR](AGENT_START_HERE.md) |
| Check the SG08 owner identity contract | [Owner protocol and identity profile](protocol/worktree.md#identity-and-lifecycle) |
| Review the syntax | [EBNF](spec/splicery.ebnf), [language semantics](spec/language.md), [lowering and IR](spec/resolved-ir.md), [worked examples](examples/README.md) |
| See the flow and ownership | [Interface and protocol](protocol/worktree.md), [worked exchange](protocol/example.json) |
| Audit type and build boundaries | [Rust, nominal types and quality](guide/engineering.md) |
| Run or extend proofs | [Lean sources and obligations](formal/README.md), [TLA+ campaign](model/README.md) |
| Decide what may ship | [Gate catalogue](qualification/gates.md), [machine registry](qualification/gates.json), [scenario matrix](qualification/scenarios.md), [proof-to-code map](qualification/implementation-map.md) |
| Understand existing dependencies | [Pinned owner contracts](guide/sources.md) |
| Reproduce the handover checks | [Evidence and limits](qualification/rehearsal.md), then run python3 -B tools/check_handover.py |

## One edit from end to end

~~~mermaid
sequenceDiagram
    participant A as Agent
    participant S as Splicery frontend
    participant W as Writfold execution
    participant K as Splicery kernel
    participant F as Owned worktree
    A->>W: Start run with plan and Weft report
    W->>F: Prepare and bind via Worktrunk
    A->>S: Inspect then submit recipe
    S->>W: Resolve scoped worktree capture
    W-->>S: Exact snapshot and handles
    S->>W: Prepare typed operations
    W->>K: Validate and render pinned input
    K-->>W: Exact candidate or refusal
    alt Guards hold at apply
        W->>F: Apply exact prepared bytes
        W-->>A: Effect receipt and fresh selection
    else Target or grant changed
        W-->>A: Refusal and bounded authorized context
    end
~~~

Splicery owns language meaning, compiler and pure edit kernel. Writfold's adapter embeds the pinned kernel, owns the actual IO and records the effect. Weft evaluates its own logical obligations. The execution profile is a capability-discovery surface, with authority derived from the run, principal, current grant and owned worktree. Authoring works without learning another host language. Source-language parsing, a new sandbox database, remote resume, arbitrary scripts and a general-purpose VM are outside this workstream.
