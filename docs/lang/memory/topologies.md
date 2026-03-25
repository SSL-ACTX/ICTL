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

## Implementation Details

The ICTL VM uses a deterministic arena-based memory model. 
1. **Consumption**: A `Valid` value transitions to `Consumed` when moved.
2. **Decay**: A `Valid` struct/topology transitions to `Decayed` when one of its fields is extracted. 
3. **Propagation**: Entanglement triggers asynchronous updates to parallel timelines, ensuring that "time-warped" dependencies are correctly handled at merge-time.
