# Proposal: Implementing and Consuming Server-Streaming RPCs

> **Status: proposal.** Not yet accepted into the design. First used by `examples/notes`.

## Problem

[11-protocols-and-rpc.md](../11-protocols-and-rpc.md) declares server streaming (`rpc watch(cartId: CartId) -> Stream<CartEvent>`) and says it works in browsers. It doesn't say how a server produces a stream, how several requests share events (one tab's save reaching another tab's stream), how the client reads a stream and stops reading it, or how tests observe one.

## Proposal

### The handler returns a `Stream<T>`

```k
rpc watch() async throws AccessError -> Stream<NoteChange> {
    let live = changes.subscribe(ownerId: session.user.id)
    let current = await notes.list(ownerId: session.user.id)

    return live.startingWith(.snapshot(current.map(summarize)))
}
```

- The handler may be `async`, to load initial data before the stream starts. It returns once; the runtime then sends each element as it is produced.
- **Guards run before the handler, as for any rpc.** A refusal, or any error the handler throws, is thrown by the client's call itself, not later from the stream.
- The stream is closed when it finishes, when the client stops reading, or when the connection drops. Closing it cancels its producer, which releases any subscription.

### `Stream<T>`

`Stream<T>` is a core type: an asynchronous sequence that is read once.

| Member | Meaning |
|---|---|
| `filter { … }`, `map { … }` | lazily transformed streams |
| `startingWith(value)` | a stream that yields `value` first, then the elements of this one |
| `next() async -> T?` | the next element, or `nil` once the stream has ended |

### Reading a stream: `for await`

```k
let changes = try await api.watch()

for await change in changes {
    apply(change: change)
}
```

- **`for await x in stream { … }`** runs the body for each element, in order, and ends when the stream ends. `break` closes the stream.
- **Ending is not an error.** A dropped connection ends the loop like a finished stream. The caller decides whether to subscribe again; a protocol that starts with a snapshot makes re-subscribing safe.
- **Cancellation closes the stream.** When the async effect or function running the loop is cancelled ([03-reactivity.md](../03-reactivity.md)), the loop stops at its current `await`, and the server-side producer is cancelled too.

### Sharing events between requests: `Broadcast<T>` and `@Scope(.server)`

Server rendering isolates service state per request ([10-services.md](../10-services.md)), so a stream for one tab can't see a save handled for another without a deliberate shared object.

- **`@Scope(.server)`** gives a service one instance per server process, shared by every request. Such a service is server-only, may be injected only by other server-only services, and must not hold per-user state in fields, only in what it publishes.
- **`Broadcast<T>`** (in `k.rpc`) is a class for fan-out:
  - `publish(value)` delivers `value` to every current subscriber. It never blocks the publisher.
  - `subscribe() -> Stream<T>` starts a subscription immediately, buffering messages until they are read, so nothing published after the call is missed.
  - A subscriber that falls more than 1,000 messages behind is closed, so one slow tab can't exhaust memory. Its stream ends, and the client re-subscribes.
- Filtering per user is ordinary stream code: `channel.subscribe().filter { owned => owned.ownerId == ownerId }`. Because the filter runs on the server, a client never receives another account's events.

### Running a stream for the life of a page

A page that follows a stream starts it from an effect with **no dependencies**:

```k
@Effect(on: [])
func followChanges() async {
    try await notes.watch()          // returns when the stream ends
}
```

- `@Effect(on: [])` runs once after the page mounts and never re-runs. Leaving the page cancels it, which closes the stream.
- Effects don't run during server rendering ([timers-and-clock.md](timers-and-clock.md)), so a streaming page renders a loading state on the server and fills in once the stream's first message arrives.

### Tests

- In integration tests, a stream is read with `await stream.next()`. Calls run in-process ([21-testing.md](../21-testing.md)), so a message published by a call has been delivered by the time that call returns.
- **`screen.settle()` treats an effect that is waiting for a stream's next element as idle,** after processing every element already delivered. Without that, a page that follows a stream could never settle. See [testing-support.md](testing-support.md).

## Why

- Returning a `Stream<T>` keeps a streaming handler as simple as a unary one, with the same guards, typed errors and injection.
- `for await` reads as prose and reuses `for … in`, so there is one loop form for collections and streams.
- An explicit process-wide scope makes shared server state rare and visible. Everything else stays isolated per request by default.

## Open questions

- **Several server processes.** `Broadcast<T>` is in-process. Running more than one server needs a `BroadcastProvider` interface, with a Postgres `LISTEN`/`NOTIFY` implementation as the default for production.
- Whether the client runtime should offer automatic re-subscription with backoff, instead of every caller writing its own.
