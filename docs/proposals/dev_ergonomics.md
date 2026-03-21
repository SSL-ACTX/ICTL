# ICTL Proposal: Developer Ergonomics & Causal Tooling

This document outlines structural language additions and tooling designed to reduce the extreme cognitive load of Entropic Memory and Temporal Pacing, making ICTL highly usable without compromising its strict deterministic guarantees.

## 1. The Entropic LSP (Visual Causality)
Writing ICTL in a plain text editor is flying blind. The biggest pain point is tracking what memory is dead. We don't just need a compiler; we need an **Entropic Language Server Protocol (LSP)**.

* **Live Entropy Shading:** Variables change color based on their state. `Valid` variables are bright. `Decayed` variables (where a field was extracted) turn yellow. `Consumed` variables are instantly grayed out and rendered with a ~~strikethrough~~ in the IDE. 
* **Temporal Hover:** Hovering over any block (`if`, `speculate`, `routine`) instantly displays the Static Analyzer's WCET (Worst-Case Execution Time) calculation.
* **Ghost State Injection:** The LSP injects inline virtual text at the end of branches showing exactly what the padding overhead will be (e.g., `// ⏳ VM pads +15ms here`).

## 2. Scoped Non-Destructive Reads (`inspect` block)
[cite_start]Currently, to read a value without decaying the parent struct or consuming it, developers must use expensive `clone()` calls[cite: 2, 4], which eat into CPU budgets and bloat code. We have `peek` for routines, but we need an inline equivalent.

* **The Feature:** The `inspect` block creates a temporary, read-only view of an entropic structure.
* **Mechanics:** Inside the block, you can read any field freely. You cannot mutate, send, or consume anything derived from the inspected variable.
* **Syntax:**
  ```ictl
  let payload = struct { a = "fieldA", b = "fieldB" }
  
  // Zero-cost read. Does NOT decay 'payload'.
  inspect(payload) {
      if (payload.a == "fieldA") {
          require System.Log(message=payload.b)
      }
  }
  
  // 'payload' is still 100% Valid and can now be sent down a channel
  chan_send pipe(payload) 
  ```

## 3. Auto-Reconciliation (`reconcile auto`)
[cite_start]Forcing developers to write manual `reconcile (x=first_wins, y=decay)` for every `if/else`, `split/merge`[cite: 3, 8], or `select` is exhausting boilerplate, especially when 90% of merges follow obvious logical paths.

* **The Feature:** The `auto` keyword allows the Static Analyzer to generate the safest reconciliation strategy based on entropic graph analysis.
* **The Rules of `auto`:**
  * If a variable is untouched in all branches: Keep `Valid`.
  * If a variable is modified in Branch A, but untouched in Branch B: Branch A wins.
  * If a variable is consumed in *any* branch: Mark as `Decayed` or `Consumed` globally (safest fallback).
  * *Conflict:* If a variable is modified differently in multiple branches, the compiler throws an error demanding manual explicit reconciliation.

## 4. Inferred Temporal Contracts (`taking _`)
Counting exact milliseconds for small helper routines or micro-speculations is tedious. If the code has no unbounded loops, the Static Analyzer already knows exactly how long it takes. 

* **The Feature:** Developers can use `_` to ask the compiler to lock in the WCET automatically.
* **Syntax:**
  ```ictl
  // The compiler calculates this takes exactly 3ms. 
  // It effectively compiles to `taking 3ms`.
  routine fast_math(peek data) taking _ {
      let x = data.value * 10
      yield x
  }
  ```
* **Constraint:** This is strictly forbidden if the routine contains network I/O or `await` calls, which inherently require human-defined temporal boundaries.

---

### Why this changes everything
With these four features, you remove the "accounting" work from the developer. The LSP handles tracking memory death, `inspect` stops the `clone()` spam, `auto` removes the boilerplate from routing, and `taking _` prevents manual counting errors. 
