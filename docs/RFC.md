# RFC: Isolated Capability Timeline Language (ICTL)

## 1. Abstract

The Isolated Capability Timeline Language (ICTL) is a deterministic, sandbox-first programming language designed around explicit time-based execution, strict capability permissions, and total isolation by default. To enforce absolute memory safety without a garbage collector or borrow checker, ICTL introduces **Entropic Memory** (destructive reads by default) and **Temporal Concurrency** (explicit timeline branching with relativistic local time). 

ICTL eliminates implicit behavior, shared mutable state, unrestricted resource access, and traditional asynchronous race conditions. It introduces **Acausal Anchors** for state rewinding, protected by **Commit Horizons** to safely manage external side-effects. ICTL is intended for high-assurance systems, untrusted code execution, and secure distributed task orchestration.

---

## 2. Design Goals

### 2.1 Security by Construction
* No implicit permissions.
* No ambient authority.
* Capabilities are declared as a static manifest at the top of an isolation block and can be parameterized with strict policy constraints (e.g., rate limits, domain restrictions).

### 2.2 Entropic Isolation (Structural Decay)
* **Destructive Reads:** Reading a variable consumes it from memory. 
* **Explicit Cloning:** Duplicating data requires explicit instructions and burns execution budget deterministically based on data size and structural depth.
* Eliminates use-after-free, double-free, and unintended data leaks at the VM level.

### 2.3 Deterministic Execution & Temporal Concurrency
* Traditional threads are replaced by explicit timeline `split` and `merge` operations.
* **Relativistic Time:** Branched timelines operate on their own local clocks. Global time is only reconciled at synchronization points (`merge`).
* **Conflict-Free Merges:** State collisions during a merge are strict compile-time errors unless an explicit resolution strategy is defined.

### 2.4 Resource Governance & Causal Containment
* CPU, memory, and IO must be explicitly enabled per isolate.
* Hard limits are enforced at the VM level.
* **Branch Failure:** If a timeline branch exceeds its budget or encounters a panic, it is immediately terminated, its memory arena is destroyed, and no side-effects escape.

---

## 3. Core Concepts

### 3.1 Timeline Execution & Relativistic Clocks
Programs are composed of time-indexed blocks. Time is treated as the primary control flow mechanism. When a timeline splits, the child branches operate on a **Relative Local Clock** that begins at `@+0ms`. 

**Time Resolution Rule:**
All time literals MUST resolve to a fixed minimum unit (e.g., nanoseconds). 
Relative clocks are normalized to global time during merge operations using deterministic rounding rules defined by the VM.

```ictl
@0ms: execute main
@50ms: split main into [fetch_data, timeout_watcher]
```

### 3.2 Parameterized Isolation Manifests
Computation occurs inside `isolate` blocks. Capabilities are declared as a manifest at the top of the block. To enforce granular security, capabilities accept strict policy parameters.

```ictl
isolate NetworkWorker {
    enable cpu(10ms)
    enable memory(1MB)
    
    // Parameterized Capabilities
    require Net.Outbound(rate=5/s, domains=["api.example.com"])
    require System.Entropy(mode="deterministic", seed=0x1A4)
}
```
*Note on Entropy:* Non-deterministic entropy (e.g., `mode="chaos"`) explicitly flags the timeline as non-replayable, disabling certain time-travel debugging guarantees.

### 3.3 Entropic Memory & Structural Decay Rules
To guarantee isolation, ICTL relies on destructive reads. A "read" is defined as any operation that transfers ownership of a value into another operation.

* **Total Consumption:** Passing a variable to a function, pushing it to a channel, or fully pattern-matching it consumes the entire object.
* **Structural Decay (Field Access):** Accessing a specific field of a struct consumes *only* that field. The parent struct remains in memory but is marked as structurally decayed. Attempting to read the parent struct whole, or the consumed field again, results in a compile-time error.

**The Clone Cost Model:**
If data must be reused, it must be explicitly cloned. `clone(x)` incurs a deterministic CPU budget penalty: `Cost = Base_Overhead + (Size_in_Bytes * C) + (Structural_Depth * K)`. If the cost exceeds the remaining isolate budget, the VM immediately triggers a temporal fault.

**Single-Ownership Invariant:**
At any point in execution, a value MUST have exactly one valid owner. Any operation that would result in multiple live references without an explicit `clone()` is a compile-time error.

### 3.4 Temporal Concurrency (Split, Merge, and Collisions)
Concurrency physically forks the memory arena (via copy-on-write or strict mitosis). 

**Merge Semantics:**
* Only explicitly yielded values survive a merge.
* If multiple branches yield a value to the same target binding, the compiler halts with a collision error.
* Developers must explicitly define a resolution strategy: `first_wins`, `priority(branch)`, or a custom reducer function.
* If a branch does not yield a value for a required binding, it is treated as yielding `void`.
* Resolution strategies MUST explicitly define behavior when encountering `void` (e.g., `first_non_void`, `fallback(value)`).

```ictl
@100ms:
  split main into [api_a, api_b]

@200ms:
  // Compile error if both yield 'user_data' unless resolved:
  merge [api_a, api_b] into main resolving (user_data = first_wins)
```

### 3.5 Acausal Anchors & Commit Horizons
Instead of `try/catch`, ICTL allows you to drop an `anchor`. 

**Causal Scope:** Anchors are strictly scoped to the *current timeline branch* and its descendant isolates. A child branch cannot rewind its parent.
**The Commit Horizon:** Because an anchor rewinds memory state, external side-effects (like a network request) could create paradoxes. To finalize external state, side-effects must be wrapped in a `commit` block. Once a `commit` executes, the VM cannot rewind past it.
**Irreversibility Guarantee:**
Once a `commit` block completes, all prior anchors in the current timeline branch become invalid for rewind operations crossing the commit boundary.

```ictl
anchor safe_point
isolate {
    let payload = format_data()
    commit {
        Net.send(payload) // This action punctures the rewind barrier
    }
    // If the next line OOMs, we rewind to safe_point, but the network request STILL HAPPENED.
    allocate_heavy_buffer() 
}
```

### 3.6 Communication Semantics
Channels are strictly entropic. Message passing is single-consumer by default. Reading from a channel consumes the message. Broadcasting requires an explicit fan-out construct that automatically applies the `clone()` cost model to the payload for every subscriber.

### 3.7 Determinism Modes

ICTL operates in two modes:

* **Deterministic Mode (default):**
  - All operations are replayable
  - Entropy must be seeded
  - Execution is fully reproducible

* **Non-Deterministic Mode:**
  - Enabled via capabilities (e.g., System.Entropy(mode="chaos"))
  - Disables rewind guarantees across affected timelines
  - Marks the execution as non-reproducible in metadata
  
---

## 4. Syntax Specification

### 4.1 Program Structure
```ebnf
program ::= timeline_block+
```

### 4.2 Timeline Block
```ebnf
timeline_block ::= "@" (time_literal | relative_time) ":" statement_block
relative_time  ::= "+" time_literal
```

### 4.3 Isolate & Manifest
```ebnf
isolate_block ::= "isolate" identifier "{" manifest statement* "}"
manifest      ::= (resource_decl | require_decl)*
require_decl  ::= "require" capability_path ("(" param_list ")")?
```

### 4.4 Temporal Control Flow
```ebnf
branch_stmt ::= "split" identifier "into" "[" identifier_list "]"
merge_stmt  ::= "merge" "[" identifier_list "]" "into" identifier ("resolving" "(" resolution_rules ")")?
anchor_stmt ::= "anchor" identifier
commit_stmt ::= "commit" "{" statement* "}"
```

---

## 5. Execution Model

1. **Static Phase:** Parse timeline blocks, validate monotonic time ordering, verify structural decay paths, and compile the parameterized capability graph.
2. **Allocation Phase:** VM initializes isolated arenas based on `enable` memory limits.
3. **Execution Phase:** VM steps through global time coordinates.
4. **Branching:** On `split`, the VM forks the memory arena. Child branches execute on relative local clocks.
5. **Reconciliation:** On `merge`, the VM pauses the fast branches until all branches reach the merge coordinate, resolves conflicts, and collapses the arenas back into the parent.

**Failure Model (Containment):**
* **Capability Violation:** Compile-time error.
* **Budget/Resource Overflow:** Immediate branch termination. The arena is destroyed. The parent timeline perceives the branch as yielding `void` or a predefined failure state. No side effects escape unless previously locked by a `commit` block.

* **Temporal Fault:**
  A deterministic runtime failure triggered when an operation exceeds the declared resource budget (e.g., CPU exhaustion from clone cost). 
  Behavior:
  - Immediate termination of the current branch
  - Memory arena destruction
  - No propagation of partially computed values
  
---

## 6. Example Program

This program demonstrates parameterized capabilities, relative time, clone budgeting, and deterministic merge resolution.

```ictl
@0ms:
  isolate init_system {
    enable cpu(10ms)
    enable memory(512KB)
    require System.Entropy(mode="deterministic", seed=0x42)
    
    let trace_id = System.generate_uuid()
    // Clone is required because we need to send it to two different branches
    let trace_id_copy = clone(trace_id) 
    
    send(trace_id) to @api_request
    send(trace_id_copy) to @watchdog
  }

@10ms:
  anchor execution_start
  split main into [api_request, watchdog]

// Branch 1: The network request
@api_request:
  @+0ms: // Local time relative to the split
    isolate {
      enable cpu(50ms)
      enable memory(2MB)
      require Net.Outbound(rate=1/s, domains=["api.backend.internal"])

      let id = receive() from main
      let payload = format("REQ: {}", id) // `id` is structurally decayed/consumed
      
      commit {
        Net.send(payload) // `payload` is consumed, side-effect finalized
      }
      
      let response = Net.await_response()
      yield response as api_result
    }

// Branch 2: Relativistic watchdog
@watchdog:
  @+60ms: // Relative to the split. If api_request takes >60ms, this triggers.
    isolate {
      enable cpu(5ms)
      require System.Control
      
      let id = receive() from main // Consume the cloned ID
      System.log("Timeout on ID: ", id)
      
      // Forcefully rewind the local branch scope, effectively cancelling the API wait
      System.rewind_to(execution_start)
      yield "TIMEOUT" as api_result
    }

@100ms: // Global time synchronization point
  // Explicit resolution required because both branches yield 'api_result'
  merge [api_request, watchdog] into main resolving (api_result = first_wins)
  
  isolate {
    require System.IO
    System.log("Final State: ", api_result) // Execution cleanly halts
  }
```
