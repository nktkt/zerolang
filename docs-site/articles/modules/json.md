## Status

Runnable today:

| API | Return | Notes |
| --- | --- | --- |
| `std.json.validate(text)` | `Bool` | Checks the current JSON subset without allocation. |
| `std.json.parse(alloc, text)` | `Maybe<JsonDoc>` | Parses with an explicit allocator and returns `null` on failure. |
| `std.json.streamTokens(text)` | `usize` | Counts stream tokens without building an owned tree. |
| `std.json.writeString(buffer, text)` | `Maybe<String>` | Writes an escaped JSON string into caller storage. |
| `std.json.decodeBoundary()` | `String` | Documents the typed decode boundary exposed by current metadata. |

Metadata labels:

- effects: parse or alloc
- allocation behavior: validation and streaming are allocation-free; parse uses explicit allocator only; writeString writes caller buffer
- target support: target-neutral
- error behavior: `Maybe` helpers return null on failure
- ownership notes: parsed documents are owned by explicit allocator storage in this compiler slice
- example: `examples/std-data-formats.0`

## Example

```zero
pub fun main(world: World) -> Void raises {
    let mut arena_buf: [16]u8 = [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]
    let mut arena = std.mem.fixedBufAlloc(arena_buf)
    let parsed = std.json.parse(arena, "{\"ok\":true}")
    let mut out: [16]u8 = [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]
    let text = std.json.writeString(out, "zero")
    if parsed.has && text.has && std.json.streamTokens("{\"ok\":true}") == 3 {
        check world.out.write("json ok\n")
    }
}
```

## Design Notes

JSON should not fake allocation-free semantics. Validation and streaming stay
allocation-free.

Parsing into an owned document requires an explicit allocator, and streaming
paths expose their storage and error behavior.
