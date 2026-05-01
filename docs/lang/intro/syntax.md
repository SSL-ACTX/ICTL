# ICTL Syntax Reference

This document provides a formal specification of the Isolate Concurrent Temporal Language (ICTL) syntax. ICTL is a research-focused language designed for deterministic, time-aware concurrency with entropic memory semantics.

---

## 1. Program Structure

An ICTL program consists of one or more **Timeline Blocks**.

### Timeline Blocks
A timeline block defines an execution context at a specific time coordinate.

**Syntax:**
```ictl
@<time_coordinate>: <statement_block>
```

**Time Coordinates:**
- **Absolute Time**: `@0ms:`, `@100ms:` (Time relative to the root timeline start).
- **Relative Time**: `@+10ms:` (Time offset relative to the current timeline entry).
- **Branch Identifier**: `@branch_name:` (Executes within a specific named branch).

**Example:**
```ictl
@0ms: {
  split main into [worker]
  @worker: {
    let x = "hello"
  }
}
```

---

## 2. Declarations and Expressions

### Variable Assignment (`let`)
Initializes a new binding in the current branch's entropic arena.

**Syntax:**
```ictl
let <identifier> = <expression>
```

**Entropic Impact:**
- If `<expression>` is a variable, it is **consumed** (moved) from its source and becomes unavailable unless wrapped in `clone()`.
- If `<expression>` is a literal, it is allocated in the local arena.

### Type Definitions (`type`)
Defines custom structures with advanced temporal and entropic constraints.

**Syntax:**
```ictl
type <name> = struct [decay_after <amount>ms] [scoped(@<branch>)] {
    <field_list>
}
```

**Advanced Features:**
- **Auto-Decay (`decay_after`)**: The variable automatically transitions to the `Decayed` state once its age (measured from instantiation) exceeds the specified duration.
- **Timeline-Scope (`scoped`)**: Restricts the type to a specific timeline branch. Any attempt to move or instantiate it in a different branch triggers a `Timeline Violation`.

**Example:**
```ictl
type SessionToken = struct decay_after 50ms {
    id: int
}
```

### Entropic Transitions (`decay_handler`)
Defines a logic block that executes automatically immediately before a variable of a specific type decays.

**Syntax:**
```ictl
decay_handler for <type_name> {
    <statements>
}
```

---

## 3. Control Flow

### Temporal Assertions (`assert_time`)
Enforces strict temporal constraints on execution.

**Syntax:**
```ictl
assert_time(elapsed <relop> <amount>ms) [else <statement_block>]
```

**Semantics:**
- **Static Analysis**: The analyzer computes the Worst-Case Execution Time (WCET). If the limit is statically exceeded, a `Temporal Assertion Violation` is thrown.
- **Dynamic Check**: The VM verifies the local clock at runtime. If the assertion fails, the `else` block is executed, or execution faults if no fallback is provided.

### Conditional Execution (`if`)
Performs speculative path evaluation followed by deterministic state reconciliation.

**Syntax:**
```ictl
if (<expression>) <statement_block> [else <statement_block>] [reconcile (<resolution_rules>)]
```

**Semantics:**
- Both branches are speculatively analyzed for entropic consistency.
- `reconcile` rules define how variables consumed in only one path are resolved.

### Speculation (`speculate`)
Creates a transient micro-timeline for trial computations with zero-leakage rollback.

**Syntax:**
```ictl
speculate (max <amount>ms) {
    <statements>
    commit { <statements> }
} [fallback { <statements> }]
```

**Semantics:**
- **Rollback**: On failure (`collapse` or timeout), the state is restored exactly to pre-speculation.
- **Commit**: On success, explicitly tagged variables (Selective mode) or the full state (Full mode) merge into the parent.

### Paced Iteration (`for`)
Iterates over collections with strict temporal and entropic constraints.

**Syntax:**
```ictl
for <item> <mode> <source> [pacing <amount>ms] [(max <amount>ms)] { <statements> }
```

**Modes:**
- `consume`: Source is destroyed; items are moved into the loop scope.
- `clone`: Source remains valid; items are cloned into the loop scope.

**Pacing:**
- Ensures each iteration takes exactly `Nms`. Overruns trigger a `WatchdogBite`.

### Parallel Mapping (`split_map`)
A scatter-gather construct spawning independent timelines for each element in a collection.

**Syntax:**
```ictl
split_map <item> <mode> <source> { <statements> } reconcile (<resolution_rules>)
```

---

## 4. Routines and Contracts

### Routine Definition (`routine`)
Defines a temporal procedure with a deterministic execution contract.

**Syntax:**
```ictl
routine <name>(<params>) taking (<amount>ms | _) { <statements> }
```

**Parameter Modes:**
- `consume`: Argument is moved into the routine.
- `clone`: Argument is copied.
- `peek`: Read-only access; caller state unchanged.
- `decay`: Caller value becomes `Decayed` after the call.

---

## 5. Timeline Management

### Branching (`split` / `merge`)
- **`split <parent> into [<branches>]`**: Spawns isolated child timelines.
- **`merge [<branches>] into <target> [resolving (<rules>)]`**: Recombines branch states.

### Resets and Anchors
- **`anchor <name>`**: Snapshots the current timeline state (clock and arena).
- **`rewind_to(<name>)`**: Restores the timeline to the specified anchor.
- **`watchdog <target> timeout <amount>ms [recovery <block>]`**: Monitors a branch and executes recovery logic on timeout.
- **`reset <branch> to <anchor>`**: Implementation-level acausal reset.

### Entropic Entanglement
Links variables across isolated timelines for zero-tick cross-branch synchronization.

**Syntax:**
```ictl
entangle(<variable_list>)
```

**Semantics:**
- Variables in the group share the same entropic state.
- If one variable is **consumed** or **decayed** in any branch, all entangled variables in all branches immediately transition to the same state.
- Entanglement must be established in the parent timeline before splitting to synchronize across the resulting branches.

---

## 6. Channels and Concurrency

### Communication
- **`open_chan <name>(<capacity>)`**: Initializes a buffered communication channel.
- **`chan_send <chan>(<value>)`**: Moves `<value>` into the channel buffer.
- **`chan_recv(<chan>)`**: Extracts a value from the channel.

### Isochronous Slicing
- **`slice <amount>ms`**: Sets a fixed-time execution slice for the current isolate.
- **`loop tick { <statements> }`**: Executes logic within a single slice, padding remaining time and committing channel buffers.

---

## 7. Diagnostics and Capabilities

### Observables
- **`print(<expression>)`**: Consuming output (standard entropic evaluation).
- **`debug(<expression>)`** / **`log(<expression>)`**: Non-consuming peek for inspection.

### Capability Manifests (`isolate`)
Sandboxes a block of code with specific resource requirements and capabilities.

**Syntax:**
```ictl
isolate [<identifier>] {
    [enable <resource>(<amount>)]
    [require <capability>(<params>)]
    [slice <amount>ms]
    <statements>
}
```

---

## 8. Low-Level / System Statements
- **`network_request <url>`**: Triggers a simulated network effect (cost: 5ms).
- **`reset <branch> to <anchor>`**: Implementation-level acausal reset.
- **`collapse`**: Immediately aborts the current speculative block.
