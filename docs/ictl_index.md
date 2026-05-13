# ICTL Documentation Hub

Welcome to the official documentation for the Isolate Concurrent Temporal Language (ICTL). This hub provides a structured entry point to the formal specifications, virtual machine internals, and original design proposals that define the ICTL ecosystem.

## 1. Formal Language Specifications (`docs/spec/`)
The following documents define the formal behavior, syntax, and semantics of ICTL.

- **[Formal Syntax Reference](./spec/ictl_spec_syntax.md)**: EBNF grammar and program structure.
- **[Core Semantic Model](./spec/ictl_spec_semantics.md)**: Operational semantics and entropic state transitions.
- **[Control Flow](./spec/ictl_spec_control_flow.md)**: Branching, speculative execution, and reconciliation.
- **[Iteration & Pacing](./spec/ictl_spec_iteration.md)**: Deterministic loops and temporal pacing.
- **[Routine Contracts](./spec/ictl_spec_routines.md)**: Procedure definitions and WCET enforcement.
- **[Speculative Branches](./spec/ictl_spec_speculation.md)**: Micro-timelines and rollback mechanisms.
- **[Type System](./spec/ictl_spec_types.md)**: Entropic types and decay constraints.
- **[Topological Field Access](./spec/ictl_spec_topologies.md)**: Memory layout and field-level entropy.
- **[Asynchronous Promises](./spec/ictl_spec_promises.md)**: Temporal promises and causal synchronization.
- **[Timeline Routing](./spec/ictl_spec_temporal_routing.md)**: Advanced routing across isolated timelines.
- **[Isochronous Scheduling](./spec/ictl_spec_isochronous_scheduling.md)**: High-precision temporal synchronization.

## 2. STVM Internals (`docs/stvm/`)
Technical documentation regarding the Stack-based Temporal Virtual Machine.

- **[Memory Reclamation](./stvm/ictl_stvm_memory_reclamation.md)**: Entropic Garbage Collection (EGC) and arena management.

## 3. Design Proposals & RFCs (`docs/proposals/`, `docs/rfc/`)
Historical design documents and the standard RFC process.

- **[Standard RFC Template](./rfc/ictl_RFC.md)**: Guidelines for proposing language changes.
- **[Entropic GC Proposal](./proposals/ictl_prop_egc.md)**: Original design for deterministic reclamation.
- **[Advanced Routing](./proposals/ictl_prop_advanced_routing.md)**: Early designs for complex timeline topologies.
- **[Developer Ergonomics](./proposals/ictl_prop_dev_ergonomics.md)**: Strategies for improving language usability.
- **[If/Else Speculation](./proposals/ictl_prop_if_else.md)**: Design for speculative conditional branches.
- **[Isochronous Matrix](./proposals/ictl_prop_isochronous_matrix.md)**: Mathematical foundations for temporal scheduling.
- **[Iterative Paced Loops](./proposals/ictl_prop_iter_paced_loop.md)**: Proposal for time-constrained iteration.
- **[Promises & Causality](./proposals/ictl_prop_promises.md)**: Design for acausal synchronization.
- **[Routine TP Contracts](./proposals/ictl_prop_routines_tp_contract.md)**: Temporal performance contracts for routines.
- **[Speculative Branches Proposal](./proposals/ictl_prop_speculative_branches.md)**: Early research on speculative execution.

---
*This index is maintained as the authoritative source for ICTL documentation.*
