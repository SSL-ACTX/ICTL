# Entropic Topologies

Entropic Topologies allow for complex, cyclically-related data structures and advanced index-based access, while maintaining ICTL's deterministic, zero-cost memory management model.

## Topology Literals

A `topology` is a collection of entropic states that can be accessed by dynamic keys (strings or integers).

```ictl
let network = topology {
    nodeA = { status = "active", load = 10 },
    nodeB = { status = "active", load = 20 }
}
```

## Index Access

Topologies and Structs support dynamic index access using the `[]` operator.

```ictl
let key = "nodeA"
let node = network[key]
```

## Entropic Entanglement

Topologies can be entangled across parallel timelines. This ensures that any destructive operation (consumption or decay) on a topology field in one branch is reflected in all entangled branches.

```ictl
@0ms: {
    let t = topology { first = 1, second = 2 }
    entangle(t)
    
    split main into [w1, w2]
    
    @w1: {
        let val = t["first"] // Decays 't', consumes 'first'
    }
    
    @w2: {
        match entropy(t["first"]) {
            Consumed: print("Field 'first' was consumed in a parallel time!")
            Valid(v): print("Field 'first' is still here.")
        }
    }
}
```

## Field Decay and Match Entropy

When a field is extracted from a topology or struct, the parent container transitions to a `Decayed` state. You can handle this state using `match entropy`.

```ictl
match entropy(my_topology) {
    Valid(t): 
        print("Topology is whole.")
    Decayed(d):
        print("Topology has been partially consumed.")
        // 'd' now holds the remaining fields
    Consumed:
        print("Topology is completely gone.")
}
```

## Relativistic Keys and Topographical Merge

### 1. Pointers are Absolute; Keys are Relativistic
In ICTL, a topology key (e.g., `"node_1"`) is a **Relativistic Key**, not a raw physical pointer. It names a semantic slot in the local timeline's arena.
- A `split` clones the causal arena state into independent child branches.
- Each branch gets its own physical value for the same key.
- The same key can diverge to different states in different branches (e.g., `Valid` in one branch and `Consumed` in another).

### 2. Relativistic Divergence in Action
```ictl
@0ms: {
  let graph = topology {
    core = struct { status = "stable", backup = "node_b" },
    node_b = struct { status = "standby", backup = null }
  }

  split main into [worker_alpha, worker_beta]

  @worker_alpha: {
    let c = graph["core"]
    c.status = "upgrading"
  }

  @worker_beta: {
    let dead_core = consume(graph["core"])
  }
}
```
- `worker_alpha` has `graph["core"]` as `upgrading`.
- `worker_beta` has consumed `graph["core"]`, creating an entropic hole.

### 3. The Topographical Merge (Collapsing the Wavefunction)
```ictl
@10ms: {
  merge [worker_alpha, worker_beta] into main resolving (
    graph: topology_union {
      "core": priority(worker_alpha),
      _: decay
    }
  )
}
```
- `topology_union`: union of keys with per-key resolution.
- `topology_intersect`: intersection requiring all branches to have valid key values.

### 4. `on_invalid` Causal Reversion (Topological Rewind)
`topology_union` / `topology_intersect` now support an optional `on_invalid` clause in the resolution block:

```ictl
merge [alpha, beta] into main resolving (
  graph: topology_union {
    "core": priority(alpha),
    _ : decay,
    on_invalid: rewind alpha to base
  }
)
```

- `on_invalid` is evaluated when the merge outcome is semantically invalid due to entropic conflicts.
- `rewind <branch> to <anchor>` means rollback the named branch execution to the anchor and retry with a consistent causality prefix.

This helps enforce deterministic conflict handling for topographical merges and supports causal reversion for robust temporal recovery.

**Why this is a Superpower**
- Graph-like data structures can evolve in each branch without cross-branch race conditions.
- No mutex/`Arc<RwLock<T>>` complexity.
- Static entropic analysis tracks state across split/merge effectively.

## Implementation Details

The ICTL VM uses a deterministic arena-based memory model. 
1. **Consumption**: A `Valid` value transitions to `Consumed` when moved.
2. **Decay**: A `Valid` struct/topology transitions to `Decayed` when one of its fields is extracted. 
3. **Propagation**: Entanglement triggers asynchronous updates to parallel timelines, ensuring that "time-warped" dependencies are correctly handled at merge-time.
