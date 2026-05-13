# ICTL Type System Specification

The Isolate Concurrent Temporal Language (ICTL) implements explicit type declarations and structural typing to provide a robust layer of semantic safety.

## Formal Type Declaration

```ictl
@0ms: {
  type Point = struct { x:int, y:int }

  let p: Point = struct { x = 5, y = 10 }
  let q = p.x + p.y
}
```

## Structural Destructuring and Iteration

The `for` loop facilitates the destructuring of structures during iterative cycles.

```ictl
@0ms: {
  let raw = struct { a = "1", b = "2" }

  for item consume raw {
    // 'item' possesses an inferred structure: { key:string, value:? }
    let key = item.key
    let value = item.value
    let combined = struct { key = key, value = value }
  }
}
```

## Routine Parameter and Return Type Enforcement

Routine signatures incorporate explicit parameter and return type specifications.

```ictl
@0ms: {
  routine add(consume a:int, consume b:int) -> int taking 2ms {
    let sum = a + b
    yield sum
  }

  let result:int = call add(3, 4)
}
```

## Static and Runtime Error Categorization

- **Type Mismatch**: Triggered by incompatible assignments or expression evaluations.
- **Field Resolution Failure**: Triggered by attempts to access non-existent fields within a structure, resulting in a `TypeMismatch` error with diagnostic metadata.
