# Services, injection and RPC

Sources: `book/apps/services.md`, `protocols.md`, `streaming.md` and `channels.md`, and
`design/10-services.md`, `11-protocols-and-rpc.md`, `28-rpc-wire-format.md`, `29-server-streaming.md`
and `44-channels.md`.

## Services

`service` is both the unit of dependency injection and the container for shared state. There's no
store library.

```k
/// The notes, kept in the browser's local storage.
public service NoteStore {
    @Inject let storage: LocalStorage

    // The key's name and the shape of `Note` are recorded in storage.lock.
    private let savedNotes = StorageKey<List<Note>>(name: "notes")

    @State var notes: List<Note> = []
    @State var problem: StorageProblem? = nil

    init() {
        restore()
    }

    // …
}
```

- `@Inject let x: T` is always `let` and always private. Injection is resolved at compile time, so
  a cycle, or an interface with a missing or ambiguous implementation, is a compile error. There's no
  constructor injection.
- **Only the service's own methods write its fields.** Components read `cart.items` but can't assign it.
- One `init()` at most: no parameters, synchronous, non-throwing. It runs once per instance, after
  injection. Record errors in state. In a client-only service it doesn't run during server rendering.
- An interface with exactly one implementation is injected automatically. With several, choose one
  with `use X for Y [in .env, …]` in `App.k`.

### Scopes

| `@Scope(…)` | Server | Browser |
|---|---|---|
| `.app` (default) | one per request or job run | one per app |
| `.request` | one per request (server-only) | — |
| `.route` | one per request | while the route is mounted |
| `.component` | one per injecting subtree | the same |
| `.server` | one per process, shared (server-only); can't inject `Session` | — |

A service may inject only services that live at least as long as it does.

## Protocols

```k
import k.auth.Authenticated
import k.rpc.RpcOnlyError

/// The signed-in customer's orders.
public protocol OrdersProtocol requires Authenticated {
    rpc recent() throws RpcOnlyError -> List<OrderSummary>
}
```

```k
import k.auth.Session
import k.rpc.RpcOnlyError

/// The server side of `OrdersProtocol`.
public service OrdersApi implements OrdersProtocol {
    @Inject let orders: OrderRepository
    @Inject let session: Session

    rpc recent() async throws RpcOnlyError -> List<OrderSummary> {
        // `Authenticated` guards the protocol, so `session.user` isn't optional here.
        let mine = await orders.recent(for: session.user.id, limit: 20)

        return mine.map { order => OrderSummary(id: order.id, total: order.total, isPaid: order.status == .paid) }
    }
}
```

```k
import k.rpc.RpcOnlyError

/// The client's way to the customer's orders.
public service OrdersService {
    @Inject let api: OrdersProtocol

    func recent() async throws RpcOnlyError -> List<OrderSummary> {
        return try await api.recent()
    }
}
```

- `protocol` is a wire contract. It compiles to Connect, Protobuf-compatible but JSON on the wire
  today. Parameters and results are `Codable`. An `interface` is in-process and something else.
- Every rpc declares `throws`. Its error enum contains `case rpcFailure(RpcFailure)` (`import
  k.rpc.RpcFailure`), or it uses `RpcOnlyError`. **Only the runtime throws `rpcFailure`**, with
  `.rejected(GuardRejection)` (`.unauthenticated`, `.forbidden`, `.rateLimited(retryAfter:)`),
  `.invalid(ValidationError)`, `.offline` and `.timeout` (browser only).
- A handler is an `rpc` or a `func` with the same name. It may add `async`. `rpc` outside a protocol
  implementation is K5508.
- Guards go after the full signature, on the protocol header or on an rpc of the protocol:
  `rpc add(request: AddItemRequest) throws CartError -> CartItem requires RateLimit(100.perMinute)`.
  A `requires` on a handler is K5507.
- `@Inject let api: SomeProtocol` in a service gives a network client in the browser, and an
  in-process call during server rendering and in tests. Guards and validation run either way.
- A `Validatable` parameter is validated before the handler runs.
- **Typed errors cross the wire**, so client `catch` stays exhaustive:

```k
do {
    try await auth.signIn(credentials: credentials.value)

    Router.navigate(to: returnTo)
} catch .invalidCredentials {
    problem = "That email and password don't match an account."
} catch .rpcFailure(.invalid(let failure)) {
    throw failure
} catch .rpcFailure(.rejected) {
    problem = "Too many attempts. Wait a minute, then try again."
} catch .rpcFailure(.offline), .rpcFailure(.timeout) {
    problem = "Couldn't reach the shop. Check your connection and try again."
}
```

- `rpc.lock` records wire names and field numbers. A breaking change fails `k check` (K8001) unless
  the protocol is marked `@Breaking("reason")`. Wire path: `POST /<project>.<module>.<Protocol>/<Rpc>`.
- Don't put server-only types or a third party's response body in an error payload.

## Server streaming

An rpc returning `Stream<T>` is read with `for await`. Share events across requests with a
`@Scope(.server)` service holding a `Broadcast<Key, T>` (from `k.rpc`):

```k
import k.rpc.Broadcast

/// One instance per server process, shared by every request.
@Scope(.server)
public service Ticker {
    private let channel: Broadcast<String, Int> = Broadcast()

    func publish(room: String, count: Int) {
        channel.publish(key: room, value: count)
    }

    func subscribe(room: String) -> Stream<Int> {
        return channel.subscribe(key: room)
    }
}
```

```k
rpc watch(room: String) async throws RpcOnlyError -> Stream<Int> {
    return ticker.subscribe(room: room).startingWith(0)
}
```

```k
func follow(room: String) async throws RpcOnlyError {
    let counts = try await api.watch(room: room)

    for await count in counts {
        latest = count
    }
}
```

A page follows a stream for as long as it's shown from `@Effect(on: [])`:

```k
@Effect(on: [])
func follow() async {
    do {
        try await counts.follow(room: room)
    } catch .rpcFailure {
        isDisconnected = true
    }
}
```

- A dropped connection ends the loop, and that isn't an error. Reconnecting is up to you. Leaving the
  page closes the stream.
- Publishing inside a transaction is deferred until the commit. With Postgres it reaches every replica.
- Subscribe before you read the snapshot, then `live.startingWith(.snapshot(current))`, so nothing is
  lost in between.
- `Stream` has `map`, `filter`, `startingWith`, `prefix(through:)` and `Stream.merge(…)`.

## Channels (two-way)

`rpc room(name: String) throws RoomError -> Channel<Command, Event, CommandError>` gives a WebSocket
with typed messages both ways, reconnection, and `Presence`. The handler returns
`Channel(sending: stream) { command => … }`. The client reads `opened.messages` with `for await`,
sends with `channel.send(…)`, and watches `channel.state` (`.connecting`, `.open`, `.reconnecting`,
`.closed`). Tests use `TestPeers`, `TestChannels` and `TestChannel`. Read `book/apps/channels.md` and
`examples/huddle` before you write one.

## Between services

In a workspace, a protocol served by another app is called through
`use remote(SomeServiceConfig) for Protocol` in `App.k`, guarded by `ServiceAuthenticated`. See
[workspaces-and-deploy.md](workspaces-and-deploy.md).
