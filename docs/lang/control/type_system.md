# Type System

ICTL now supports explicit type declarations and structural typing as a safety layer.

## Type declaration

```ictl
@0ms: {
  type Point = struct { x:int, y:int }

  let p: Point = struct { x = 5, y = 10 }
  let q = p.x + p.y
}
```

## For-loop destructuring and struct iteration

```ictl
@0ms: {
  let raw = struct { a = "1", b = "2" }

  for item consume raw {
    // 'item' has inferred structure { key:string, value:? }
    let key = item.key
    let value = item.value
    let combined = struct { key = key, value = value }
  }
}
```

## Routine parameter and return types

```ictl
@0ms: {
  routine add(consume a:int, consume b:int) -> int taking 2ms {
    let sum = a + b
    yield sum
  }

  let result:int = call add(3, 4)
}
```

## Error handling

- Mismatched assignment or expression types report `TypeMismatch`.
- Missing fields on struct access report `TypeMismatch` with a field not found message.
