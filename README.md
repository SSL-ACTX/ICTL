<div align="center">

![ICTL Banner](https://capsule-render.vercel.app/api?type=waving&color=0:0ea5e9,100:0284c7&height=220&section=header&text=ICTL&fontSize=80&fontColor=ffffff&animation=fadeIn&fontAlignY=35&desc=Isolate%20Concurrent%20Temporal%20Language&descSize=20&descAlignY=55)

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg?style=for-the-badge)](https://opensource.org/licenses/MIT)
[![Rust](https://img.shields.io/badge/Rust-1.70%2B-orange.svg?style=for-the-badge)](https://www.rust-lang.org/)
[![Architecture](https://img.shields.io/badge/Architecture-Temporal_VM-magenta.svg?style=for-the-badge)]()
[![Status](https://img.shields.io/badge/Status-Development-purple.svg?style=for-the-badge)](https://github.com/SSL-ACTX/ictl)

[**Documentation**](#documentation-index) &middot; [**Core Concepts**](#the-three-pillars) &middot; [**Getting Started**](#getting-started)

</div>

---

**ICTL** (Isolate Concurrent Temporal Language) is a research-focused, Rust-based language designed for **deterministic, time-aware concurrency**. 

Unlike traditional languages that rely on non-deterministic threads and shared mutable state, ICTL models computation through **isolated timelines**, **entropic memory**, and **explicit temporal effects**. Every instruction costs time, every move has an entropic consequence, and every parallel branch is analytically reconciled.

---

## The Three Pillars

### ⏳ 1. Deterministic Timing
In ICTL, time is not a side effect—it's a first-class primitive. Every statement has a defined temporal cost. The VM ensures that execution is time-invariant through **deterministic padding**, making race conditions mathematically impossible.

### 🍃 2. Entropic Memory
Memory follows the laws of entropy. When a value is moved or a structure is accessed, its "state" changes. Field access "decays" a struct, preventing it from being moved as a whole. This eliminates entire classes of memory safety bugs without a traditional borrow checker.

### 🗺️ 3. Isolated Timelines
Concurrency is achieved by `split`-ing the current timeline into independent branches. Each branch has its own arena and clock. Merging timelines requires explicit `reconcile` rules to resolve conflicts acausally.

---

## Language Highlights

- **Entropic Topologies**: Complex, cyclically-related data structures that propagate decay across entangled timelines.
- **Acausal Resets**: Use `anchor` and `watchdog` to rewind failing branches to previous valid states.
- **Speculative Trials**: Execute sensitive logic in a `speculate` block with O(1) rollback on failure.
- **Isochronous Scheduling**: Build real-time control loops with `loop tick` and double-buffered channel semantics.
- **Paced Iteration**: Process data with strict `pacing` and temporal budgets.

---

## Documentation Index

Explore the language internals and syntax in the `docs/lang/` directory:

| Category                            | Topics Covered                                                                                                                                                                                         |
| :---------------------------------- | :----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **[Intro](./docs/lang/intro/)**     | [Overview](./docs/lang/intro/overview.md), [Syntax](./docs/lang/intro/syntax.md), [Semantics](./docs/lang/intro/semantics.md)                                                                          |
| **[Control](./docs/lang/control/)** | [Branching & Match](./docs/lang/control/branching.md), [For & Map Loops](./docs/lang/control/loops.md), [Speculation](./docs/lang/control/speculation.md), [Routines](./docs/lang/control/routines.md) |
| **[Memory](./docs/lang/memory/)**   | [Topologies & Fields](./docs/lang/memory/topologies.md), [Promises & Await](./docs/lang/memory/promises.md), [Garbage Collection](./docs/lang/memory/gc.md)                                            |
| **[Timing](./docs/lang/timing/)**   | [Advanced Routing](./docs/lang/timing/advanced_routing.md), [Isochronous Ticks](./docs/lang/timing/isochronous_matrix.md)                                                                              |

---

## Getting Started

### Prerequisites
- [Rust Toolchain](https://www.rust-lang.org/tools/install) (1.70+)

### Installation & Execution

```bash
# Clone and enter
git clone https://github.com/SSL-ACTX/ictl.git
cd ictl

# Build the toolchain
cargo build

# Run semantic analysis + execution
cargo run -- --run examples/sample.ictl
```

### CLI Flags
- `--check` : Perform static semantic analysis only.
- `--run`   : Execute the program after analysis (default).
- `--dump-ir` : Print the lowered intermediate representation.
- `--dump-ast`: Print the raw abstract syntax tree.

---

## Quick Code Example: The Sentinel Pattern

A watchdog intervenes if a worker timeline exceeds its temporal budget, resetting it to a safe anchor.

```ictl
@0ms: {
  split main into [worker]

  @worker: {
    anchor safe_checkpoint
    let task = do_complex_calculation()
    // If this takes too long, the watchdog below triggers
  }
}

@10ms: {
  watchdog worker timeout 5ms recovery {
    require System.Log(message="Temporal budget exceeded. Rewinding...")
    reset worker to safe_checkpoint
  }
}
```

---

## Technical Architecture

ICTL is implemented in Rust with a custom Stack-based Temporal VM.
- **Parser**: Powered by [Pest](https://pest.rs/) for formal grammar enforcement.
- **Analyzer**: Multi-pass static analysis for entropic safety and WCET (Worst-Case Execution Time) estimation.
- **GC**: Delta-based arena reclamation triggered on branch collapse or merge.

---

## License

MIT License - Copyright (c) 2026 SSL-ACTX / ICTL Contributors.
