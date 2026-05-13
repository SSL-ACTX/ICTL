# ICTL: Isolate Concurrent Temporal Language

## Abstract

The Isolate Concurrent Temporal Language (ICTL) is a domain-specific research language designed to address the inherent non-determinism in concurrent systems. By treating time as a first-class execution primitive and implementing an entropic memory model, ICTL provides a framework where race conditions are eliminated through mathematical enforcement of temporal invariants. This repository contains the reference implementation of the ICTL toolchain, including the compiler and the Stack-based Temporal Virtual Machine (STVM).

## Theoretical Framework

The design of ICTL is predicated on three primary architectural pillars:

### 1. Deterministic Temporal Execution
In ICTL, computational time is not an emergent side effect of hardware execution but a defined primitive within the language semantics. Every instruction is assigned a deterministic temporal cost. The STVM ensures time-invariance across disparate execution environments through deterministic padding and isochronous scheduling, effectively rendering race conditions impossible.

### 2. Entropic Memory Management
ICTL employs an "entropic" memory model based on the principle of state decay. Accessing or moving data structures results in entropic transformation, where field-level access propagates decay to the parent structure. This model eliminates the necessity for a traditional borrow checker while maintaining strict memory safety and preventing unauthorized concurrent access to mutable state.

### 3. Isolated Timeline Concurrency
Concurrency is modeled through the explicit bifurcation of execution timelines. The `split` operation generates independent execution branches, each equipped with its own memory arena and temporal clock. Synchronization is achieved through formal `reconcile` rules, which resolve potential acausal conflicts during timeline merger.

## Technical Specifications

- **Implementation Language**: Rust (1.70+)
- **Grammar**: Formal EBNF enforcement via the Pest parser generator.
- **Analysis**: Multi-pass static semantic analysis, incorporating entropic safety verification and Worst-Case Execution Time (WCET) estimation.
- **Virtual Machine**: Custom stack-based architecture with support for speculative execution and acausal resets.
- **Garbage Collection**: Delta-based arena reclamation triggered by timeline collapse or reconciliation.

## Documentation Index

Detailed specifications and technical references are maintained in the `docs/` directory. For a comprehensive overview, see the **[ICTL Documentation Hub](./docs/ictl_index.md)**.

| Category | Primary References |
| :--- | :--- |
| **Specifications** | [Formal Syntax](./docs/spec/ictl_spec_syntax.md), [Semantic Model](./docs/spec/ictl_spec_semantics.md), [Control Flow](./docs/spec/ictl_spec_control_flow.md) |
| **Execution** | [Iteration & Pacing](./docs/spec/ictl_spec_iteration.md), [Routine Contracts](./docs/spec/ictl_spec_routines.md), [Speculative Branches](./docs/spec/ictl_spec_speculation.md) |
| **Memory** | [Entropic Types](./docs/spec/ictl_spec_types.md), [Topological Access](./docs/spec/ictl_spec_topologies.md), [Memory Reclamation](./docs/stvm/ictl_stvm_memory_reclamation.md) |
| **Temporal Logic** | [Timeline Routing](./docs/spec/ictl_spec_temporal_routing.md), [Isochronous Scheduling](./docs/spec/ictl_spec_isochronous_scheduling.md) |
| **Design** | [Proposals](./docs/proposals/), [RFC Process](./docs/rfc/ictl_RFC.md) |


## System Requirements and Installation

### Prerequisites
- Rust Toolchain (Stable)

### Build Instructions
```bash
git clone https://github.com/SSL-ACTX/ictl.git
cd ictl
cargo build --release
```

### Execution Interface
The toolchain provides a command-line interface for analysis and execution:
```bash
cargo run -- --run examples/sample.ictl
```

#### Primary CLI Arguments
- `--check`: Perform static semantic and temporal analysis without execution.
- `--run`: Execute the provided source file following successful analysis.
- `--dump-ir`: Output the lowered intermediate representation for debugging.
- `--dump-ast`: Output the abstract syntax tree.
- `--trace-entropy`: Provide a per-instruction diagnostic map of memory decay.

## Exemplary Pattern: Temporal Watchdog

The following implementation demonstrates the use of temporal anchors and watchdogs to enforce execution budgets within an isolated timeline.

```ictl
@0ms: {
  split main into [worker]

  @worker: {
    anchor safe_checkpoint
    let result = execute_computation()
  }
}

@10ms: {
  watchdog worker timeout 5ms recovery {
    require System.Log(message="Temporal budget violation: initiating recovery.")
    reset worker to safe_checkpoint
  }
}
```

In ICTL, capabilities such as logging must be explicitly declared. Isolated timelines require the `System.Log` capability to be present in the manifest; failure to provide this results in a `TemporalError::MissingCapability` at runtime.

## License

This project is licensed under the GNU Affero General Public License v3.0 (AGPL-3.0). See the [LICENSE](LICENSE).

---

<div align="center">

Copyright (c) 2026 SSL-ACTX / ICTL

</div>
