## Status

Runnable today:

| API | Return | Notes |
| --- | --- | --- |
| `std.http.parseMethod(text)` | `HttpMethod` | Parses a small HTTP method token. |
| `std.http.client(net)` | `HttpClient` | Creates hosted client metadata from a network capability. |
| `std.http.server(net, address)` | `HttpServer` | Creates hosted server metadata from a network capability and address. |
| `std.http.bodyLen(bytes)` | `usize` | Reports body byte length without allocation. |
| `std.http.tlsBoundary()` | `String` | Names the platform or C-library TLS boundary. |

Metadata labels:

- effects: net or memory
- allocation behavior: no allocation
- target support: parsing and body length are target-neutral; client/server require a net-capable target
- error behavior: infallible helpers
- ownership notes: HTTP helpers borrow network capability metadata
- example: `conformance/native/pass/std-net-http-breadth.0`

## Example

```zero
pub fun main(world: World) -> Void raises {
    let net = std.net.host()
    let addr = std.net.address("localhost", 8080_u16)
    let _client = std.http.client(net)
    let _server = std.http.server(net, addr)
    let method = std.http.parseMethod("GET")
    if method == std.http.parseMethod("GET") &&
        std.http.bodyLen(std.mem.span("body")) == 4 {
        check world.out.write("http ok\n")
    }
}
```

## Design Notes

`std.http` currently exposes parsing and hosted metadata helpers. It does not
provide a request/response runtime in the current public surface.
