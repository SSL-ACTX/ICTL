# Proposal: Developer Ergonomics and Causal Diagnostics

This document specifies structural language enhancements and diagnostic tooling designed to mitigate the cognitive load associated with Entropic Memory and Temporal Pacing, ensuring the usability of ICTL without compromising its deterministic guarantees.

## 1. Entropic Language Server Protocol (Visual Causality)
Managing entropic state manually increases the probability of developmental errors. The **Entropic Language Server Protocol (LSP)** provides real-time visualization of variable states.

* **Live Entropic Shading**: Variables are visually distinguished based on their current entropic state. `Valid` variables are highlighted; `Decayed` variables (where fields have been extracted) are marked with secondary identifiers; and `Consumed` variables are visually neutralized (e.g., via strikethrough) within the development environment.
* **Temporal Metadata Hover**: Hovering over execution blocks (`if`, `speculate`, `routine`) provides an immediate display of the Static Analyzer's Worst-Case Execution Time (WCET) calculations.
- **Deterministic Padding Visualization**: The LSP provides inline annotations indicating where the Virtual Machine will inject temporal padding (e.g., `// [Padding: +15ms]`).

## 2. Scoped Non-Destructive Inspection (`inspect`)
Currently, reading a value without triggering structural decay or consumption requires explicit `clone()` operations, which consume CPU budget and increase code complexity. The `inspect` construct provides a non-destructive alternative.

* **Functional Requirement**: The `inspect` block facilitates a temporary, read-only view of an entropic structure.
* **Execution Mechanics**: Within the block, constituent fields may be read without entropic impact. Mutation, transmission, or consumption of the inspected variable is prohibited within this scope.
* **Technical Implementation Example**:
  ```ictl
  let payload = struct { a = "field_A", b = "field_B" }
  
  // Non-destructive read operation. Entropic state of 'payload' remains 'Valid'.
  inspect(payload) {
      if (payload.a == "field_A") {
          System.Log(message=payload.b)
      }
  }
  
  // 'payload' maintains structural integrity and may be transmitted.
  chan_send data_pipe(payload) 
  ```

## 3. Automated Entropic Reconciliation (`reconcile auto`)
Requiring manual specification of reconciliation protocols for every branching or merge event increases boilerplate and development complexity.

* **Functional Requirement**: The `auto` keyword facilitates the automated generation of reconciliation strategies based on entropic graph analysis.
* **Reconciliation Heuristics**:
  - Variables unmodified across all branches maintain their `Valid` state.
  - Variables modified in a single branch adopt the state of that branch.
  - Variables consumed in any branch are transitioned to the `Decayed` or `Consumed` state globally (conservative fallback).
  - **Conflict Resolution**: Divergent modifications across multiple branches trigger a compiler error, requiring explicit manual reconciliation.

## 4. Inferred Temporal Execution Contracts (`taking _`)
Manually calculating exact execution durations for minor routines or speculations is inefficient. For code with deterministic iteration bounds, the Static Analyzer can calculate the WCET automatically.

* **Functional Requirement**: Developers may utilize the `_` placeholder to instruct the compiler to calculate and enforce the WCET automatically.
* **Technical Implementation Example**:
  ```ictl
  // The compiler calculates the WCET (e.g., 3ms) and establishes the contract.
  routine calculate_offset(peek metadata) taking _ {
      let offset = metadata.value * 10
      yield offset
  }
  ```
* **Constraint**: Automated inference is prohibited for routines containing external I/O or `await` operations, which require human-defined temporal boundaries.

---

### Architectural Significance
These enhancements reduce the administrative burden on the developer. The LSP facilitates entropic state tracking, the `inspect` primitive eliminates redundant cloning, `auto` reconciliation reduces boilerplate, and inferred contracts minimize manual temporal accounting errors.
