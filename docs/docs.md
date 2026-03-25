<div align="center">

# 📘 ICTL Documentation Hub

Welcome to the comprehensive documentation for the **Isolate Concurrent Temporal Language** (ICTL). 
ICTL is built for systems where time and memory are not just resources, but explicit laws of the language.

</div>

---

## 🚀 The Learning Path

If you are new to ICTL, we recommend following this sequential learning path to master its deterministic and entropic concepts:

1.  **[Language Overview](./lang/intro/overview.md)**: The philosophy of isolated timelines and local clocks.
2.  **[Basic Syntax](./lang/intro/syntax.md)**: Variables, assignments, and structural literals.
3.  **[Entropic Semantics](./lang/intro/semantics.md)**: How values flow and consume themselves.
4.  **[Branching](./lang/control/branching.md)**: Using `if`/`else` and the powerful `match entropy` statement.
5.  **[Deterministic Timing](./lang/timing/advanced_routing.md)**: Navigating time coordinates and `@` blocks.

---

## 🏛️ Conceptual Guides

### Control & Execution
- **[Loops & Parallelism](./lang/control/loops.md)**: Master sequential `for` loops, fixed-frequency `loop`, and parallel `split_map`.
- **[Speculation](./lang/control/speculation.md)**: Run trial computations with zero-impact rollback and `commit` controls.
- **[Routines](./lang/control/routines.md)**: Encapsulate logic with temporal contracts and entropic parameter modes.

### Memory & State
- **[Topologies & Fields](./lang/memory/topologies.md)**: Create complex, entangled data structures and handle structural decay.
- **[Promises & Await](./lang/memory/promises.md)**: Handle asynchronous-style deferred effects within deterministic timelines.
- **[Garbage Collection](./lang/memory/gc.md)**: Understand how ICTL manages branch arenas and reclaims memory.

### Real-Time Scheduling
- **[Isochronous Matrix](./lang/timing/isochronous_matrix.md)**: Build high-frequency control loops using `slice` and `loop tick`.

---

## 🧱 The Standard Library (Capabilities)

ICTL interacts with the outside world through a **Capability System**. Every capability call must be explicitly declared in an isolate's manifest.

| Path                  | Parameters        | Description                                                                  |
| :-------------------- | :---------------- | :--------------------------------------------------------------------------- |
| `System.Log`          | `message: String` | Traditional logging to the host terminal.                                    |
| `System.NetworkFetch` | `url: String`     | Initiates a `defer` promise to fetch external data.                          |
| `System.Entropy`      | `mode: "chaos"`   | Disables rewinds for the current branch to permit non-deterministic entropy. |

---

## 🛠️ Tooling & Runtime

### CLI Usage
```bash
# Analyze and Execute
ictl --run program.ictl

# Static Analysis Only (Checks for entropic and temporal violations)
ictl --check program.ictl

# Debugging: Dump lowered IR
ictl --dump-ir program.ictl
```

### Static Analysis Errors
*   **Compile-Time Entropic Violation**: A variable was accessed after it was consumed or moved in a parallel path.
*   **Temporal Violation**: A routine or block body exceeded its declared time budget (WCET violation).
*   **Merge Collision**: A variable produced in multiple parallel branches requires an explicit `reconcile` rule.

---

## 🎓 Advanced Topics

- **Acausal Resets**: Using `anchor` and `watchdog` to implement self-healing temporal logic.
- **Phase-Committed Channels**: Synchronizing data transfer on isochronous tick boundaries.
- **Speculative Mode**: Comparing `selective` vs `full` commit strategies for micro-timelines.

---

<div align="center">

*Documentation is version-aligned with Ictl Toolchain v0.1.0*

</div>
