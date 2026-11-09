# Server streaming

A server-streaming rpc's handler **returns a `Stream<T>`**, the client reads it with **`for await`**, and
requests share events through a keyed **`Broadcast`** held by a process-wide service.

```k
// examples/notes/src/notes/NotesProtocol.k
public protocol NotesProtocol requires Authenticated {
    // ...
    rpc watch() throws RpcOnlyError -> Stream<NoteChange>
    // ...
}
```

```k
// examples/notes/src/notes/NotesApi.k
public service NotesApi implements NotesProtocol {
    @Inject let notes: NoteRepository
    @Inject let tags: TagRepository
    @Inject let changes: NoteChanges
    @Inject let session: Session
    // ...
    rpc watch() async throws RpcOnlyError -> Stream<NoteChange> {
        let ownerId = session.user.id

        // Subscribe before reading, so a change saved in between is delivered
        // after the snapshot instead of being lost.
        let live = changes.subscribe(ownerId: ownerId)
        let current = await notes.list(ownerId: ownerId)

        return live.startingWith(.snapshot(current.map(summarize)))
    }
    // ...
}
```

```k
// examples/notes/src/notes/NoteChanges.k
@Scope(.server)
public service NoteChanges {
    private let channel: Broadcast<AccountId, NoteChange> = Broadcast()
    // ...
    func publish(ownerId: AccountId, change: NoteChange) {
        channel.publish(key: ownerId, value: change)
    }
    // ...
    func subscribe(ownerId: AccountId) -> Stream<NoteChange> {
        return channel.subscribe(key: ownerId)
    }
}
```

## The handler

- The handler may be `async`, to load initial data before the stream starts. It returns once; the runtime
  then sends each element as it is produced.
- **Guards run before the handler, as for any rpc.** A refusal, a network failure before the stream opens
  (`rpcFailure(.offline)`, `.timeout`), or any error the handler throws, is thrown by the client's call
  itself, not later from the stream.
- The stream is closed when it finishes, when the client stops reading, or when the connection drops.
  Closing it cancels its producer, which releases any subscription.
- **A stream ends when its session ends.** The runtime closes every stream opened under a session on
  sign-out, expiry or `endAll`, so a signed-out tab stops receiving data even though guards ran only when
  it opened. The end tells the browser the session is gone, so a page that requires a user is left
  before its loop can open the stream again.

## `Stream<T>`

`Stream<T>` is a core type: an asynchronous sequence that is read once.

| Member | Meaning |
|---|---|
| `filter { … }`, `map { … }` | lazily transformed streams |
| `startingWith(value)` | a stream that yields `value` first, then the elements of this one |
| `Stream.merge(first, second)` | a stream of the elements of both as they arrive, ending when both have (one handler following two broadcasts) |
| `next() async -> T?` | the next element, or `nil` once the stream has ended |

## Reading a stream: `for await`

```k
// examples/notes/src/notes/NotesService.k
func watch() async throws RpcOnlyError {
    while true {
        let changes = try await api.watch()

        for await change in changes {
            apply(change: change)
        }

        isLive = false
    }
}
```

- **`for await x in stream { … }`** runs the body for each element, in order, and ends when the stream
  ends. `break` closes the stream.
- `for await` iterates only a `Stream`, and only in an `async` function. It takes no `key` clause.
- **Ending is not an error.** A dropped connection ends the loop like a finished stream. The caller
  decides whether to subscribe again; a protocol that starts with a snapshot makes re-subscribing safe.
- **Cancellation closes the stream.** When the effect or function running the loop is cancelled, the loop
  stops at its current `await`, and the server-side producer is cancelled too.

### Following a stream for the life of a page

A page starts the stream from an effect, which runs after mount and is cancelled when the page is left.
With `@Effect(on: [])` it runs once; the notes page lists `connectAttempt`, a counter its "Reconnect"
button increments, so the effect also re-runs on demand. Effects can't throw, so it handles errors
itself:

```k
// examples/notes/src/notes/Notes.k
@Effect(on: [connectAttempt])
func followChanges() async {
    isDisconnected = false

    do {
        try await notes.watch()
    } catch .rpcFailure(.rejected) {
        Router.navigate(to: .signIn.returnTo(.notes))
    } catch .rpcFailure {
        isDisconnected = true
    }
}
```

Effects don't run during server rendering, so a streaming page renders its initial state on the server
and fills in when the first message arrives.

### One connection per stream, shared across tabs

Each open stream holds an HTTP request, and browsers allow only six connections per origin over
HTTP/1.1, so serve HTTP/2 at the proxy ([Deployment](deployment.md#behind-a-proxy)).

> **Planned** (stream-tab-sharing). Identical streams (same rpc, same arguments, same session) opened in
> several tabs will share one connection: one tab holds it and relays the elements to the others, and
> another tab takes over when it closes. Today each tab opens its own.

## Sharing events between requests

Server rendering isolates service state per request, so a stream for one tab can't see a save handled
for another without a deliberate shared object.

- **`@Scope(.server)`** gives a service one instance per server process, shared by every request. Such a
  service is server-only, and may inject only other `.server` services and stateless injectables. It
  can't inject `Session` or anything request-scoped, so it can't hold one user's state.
- **`Broadcast<Key, T>`** (in `k.rpc`) fans values out by key:
  - `publish(key:value:)` delivers `value` to every current subscriber of `key`. It never blocks the
    publisher.
  - `subscribe(key:) -> Stream<T>` starts a subscription immediately, buffering messages until they are
    read, so nothing published after the call is missed.
  - **Filtering happens at publish time,** so a subscriber only ever buffers its own key's messages, and
    a client never receives another account's events.
  - A subscriber that falls more than 1,000 of its own messages behind is closed, so one slow tab can't
    exhaust memory. Its stream ends, and the client re-subscribes.
  - `Key` must be `Hashable` and `Codable`, and `T` must be `Codable`.
- **Publishing inside a transaction is deferred to the outermost commit,** like enqueueing a job, and
  discarded on rollback. Other tabs never see a change that was rolled back.
- **Delivery spans replicas with Postgres:** a `Broadcast` reaches the subscribers of the publishing
  process at once and those of the other replicas over `LISTEN`/`NOTIFY`
  ([Channels](channels.md#several-replicas)). With SQLite or no database an app is one process, and
  delivery stays in it.

## Tests

- In integration tests, a stream is read with `await stream.next()`. Calls run in-process, so a message
  published by a call has been delivered by the time that call returns.
- **`screen.settle()` treats an effect that is waiting for a stream's next element as idle,** after
  processing every element already delivered. Without that, a page that follows a stream could never
  settle.

> **Open question.** Whether the client runtime should re-subscribe automatically, with backoff, instead
> of every caller writing its own loop.
