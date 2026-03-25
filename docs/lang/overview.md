# ICTL: Isolate Concurrent Temporal Language

ICTL is a research-focused programming language designed for deterministic, time-aware concurrency with unique "Entropic" memory semantics.

## Core Philosophical Pillars

| Pillar                   | Description                                                                                                  |
| :----------------------- | :----------------------------------------------------------------------------------------------------------- |
| **Entropic Memory**      | Values are not static; they move, decay, and are consumed by computation.                                    |
| **Timeline Isolation**   | Concurrency is modeled as isolated branches with explicit split and merge logic.                             |
| **Temporal Determinism** | Every operation has a deterministic cost; time is a first-class citizen controlled by budgets and contracts. |
| **Causal Safety**        | Speculative branches and acausal resets (watchdogs/anchors) enable robust self-healing patterns.             |

---

## Language Highlights

- **`for` / `split_map`**: Paced iteration with explicit consumption or cloning.
- **`speculate` / `fallback`**: Transaction-style code blocks with zero-leakage rollback.
- **`routine` / `call`**: Temporal contracts with worst-case execution time (WCET) enforcement.
- **`slice` / `loop tick`**: Isochronous scheduling for high-precision timing applications.
- **`match entropy` / `await`**: Direct branching on a value's entropic state (Valid/Pending/Decayed/Consumed).

---

## Why ICTL?

ICTL is engineered for environments where **time-bound correctness** is critical. By treating memory as entropic and time as deterministic, it eliminates nondeterminism from concurrency and provides a formal model for reasoning about resource consumption and temporal effects.

> For a detailed syntax and semantic guide, please refer to the [Syntax Reference](syntax.md) and [Core Semantics](semantics.md).
