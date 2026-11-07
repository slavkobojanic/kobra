# Server Streaming

## Decision

A server-streaming rpc's handler **returns a `Stream<T>`**, the client reads it with **`for await`**, and requests share events through a keyed **`Broadcast`** held by a process-wide service.

```k
protocol NotesProtocol requires Authenticated {
    rpc watch() throws AccessError -> Stream<NoteChange>
}

service NotesApi implements NotesProtocol {
    @Inject let session: Session
    @Inject let notes: NoteRepository
    @Inject let changes: NoteChanges

    rpc watch() async throws AccessError -> Stream<NoteChange> {
        let live = changes.subscribe(ownerId: session.user.id)     // subscribe first, so nothing is missed
        let current = await notes.list(ownerId: session.user.id)

        return live.startingWith(.snapshot(current.map(summarize)))
    }
}

@Scope(.server)
service NoteChanges {
    private let channel: Broadcast<AccountId, NoteChange> = Broadcast()

    func publish(ownerId: AccountId, change: NoteChange) {
        channel.publish(key: ownerId, value: change)
    }

    func subscribe(ownerId: AccountId) -> Stream<NoteChange> {
        return channel.subscribe(key: ownerId)
    }
}
```

## The handler

- The handler may be `async`, to load initial data before the stream starts. It returns once; the runtime then sends each element as it is produced.
- **Guards run before the handler, as for any rpc.** A refusal, a network failure before the stream opens (`rpcFailure(.offline)`, `.timeout`), or any error the handler throws, is thrown by the client's call itself, not later from the stream.
- The stream is closed when it finishes, when the client stops reading, or when the connection drops. Closing it cancels its producer, which releases any subscription.
- **A stream ends when its session ends.** The runtime closes every stream opened under a session on sign-out, expiry or `endAll` ([26-auth-and-sessions.md](26-auth-and-sessions.md#lifetime-and-revocation)), so a signed-out tab stops receiving data even though guards ran only when it opened.

## `Stream<T>`

`Stream<T>` is a core type: an asynchronous sequence that is read once.

| Member | Meaning |
|---|---|
| `filter { … }`, `map { … }` | lazily transformed streams |
| `startingWith(value)` | a stream that yields `value` first, then the elements of this one |
| `next() async -> T?` | the next element, or `nil` once the stream has ended |

## Reading a stream: `for await`

```k
let changes = try await api.watch()

for await change in changes {
    apply(change: change)
}
```

- **`for await x in stream { … }`** runs the body for each element, in order, and ends when the stream ends. `break` closes the stream.
- `for await` iterates only a `Stream<T>`, and only in an `async` function. A plain `for` over a `Stream` is a compile error with a quick fix that adds `await`. It takes no `key` clause.
- **Ending is not an error.** A dropped connection ends the loop like a finished stream. The caller decides whether to subscribe again; a protocol that starts with a snapshot makes re-subscribing safe.
- **The rpc timeout covers opening the stream only.** The call that opens it fails with `rpcFailure(.timeout)` when no response arrives in time; once it is open, a stream may stay quiet for as long as nothing happens.
- **Cancellation closes the stream.** When the async effect or function running the loop is cancelled ([03-reactivity.md](03-reactivity.md)), the loop stops at its current `await`, and the server-side producer is cancelled too.

### Following a stream for the life of a page

A page starts the stream from an effect with no dependencies, which runs once after mount and is cancelled when the page is left. Effects can't throw, so it handles the error itself:

```k
@Effect(on: [])
func followChanges() async {
    do {
        try await notes.watch()          // returns when the stream ends
    } catch .rpcFailure(.rejected) {
        Router.navigate(to: .signIn)
    } catch let other {
        problem = other
    }
}
```

Effects don't run during server rendering ([03-reactivity.md](03-reactivity.md#where-effects-run)), so a streaming page renders its initial state on the server and fills in when the first message arrives.

### One connection per stream, shared across tabs

Each open stream holds an HTTP request. Browsers allow only six connections per origin over HTTP/1.1, so **identical streams (same rpc, same arguments, same session) opened in several tabs share one connection**: one tab holds it and relays the elements to the others with `BroadcastChannel`, and another tab takes over when it closes. Deployments should still serve HTTP/2 at the proxy ([38-deployment.md](38-deployment.md#behind-a-proxy)).

## Sharing events between requests

Server rendering isolates service state per request ([10-services.md](10-services.md#scopes)), so a stream for one tab can't see a save handled for another without a deliberate shared object.

- **`@Scope(.server)`** gives a service one instance per server process, shared by every request. Such a service is server-only, and may inject only other `.server` services and stateless injectables. It can't inject `Session` or anything request-scoped ([10-services.md](10-services.md#scope-compatibility)), so it can't hold one user's state.
- **`Broadcast<Key, T>`** (in `k.rpc`) fans values out by key:
  - `publish(key:value:)` delivers `value` to every current subscriber of `key`. It never blocks the publisher.
  - `subscribe(key:) -> Stream<T>` starts a subscription immediately, buffering messages until they are read, so nothing published after the call is missed.
  - **Filtering happens at publish time,** so a subscriber only ever buffers its own key's messages, and a client never receives another account's events.
  - A subscriber that falls more than 1,000 of its own messages behind is closed, so one slow tab can't exhaust memory. Its stream ends, and the client re-subscribes.
  - `Key` must be `Hashable` and `Codable`, and `T` must be `Codable`.
- **Publishing inside a transaction is deferred to the app-level commit,** like enqueueing a job, and discarded on rollback ([19-persistence.md](19-persistence.md#database-and-transactions)). Other tabs never see a change that was rolled back.
- **Several server processes:** `Broadcast` delivers through a `BroadcastProvider`. The stdlib default is in-process with SQLite and Postgres `LISTEN`/`NOTIFY` with Postgres, so replicas behind a load balancer reach each other's subscribers with no configuration.

## Tests

- In integration tests, a stream is read with `await stream.next()`. Calls run in-process ([21-testing.md](21-testing.md)), and every test's transaction is invisible to the app, so a message published by a call has been delivered by the time that call returns.
- **`screen.settle()` treats an effect that is waiting for a stream's next element as idle,** after processing every element already delivered. Without that, a page that follows a stream could never settle.

## Why

- Returning a `Stream<T>` keeps a streaming handler as simple as a unary one, with the same guards, typed errors and injection.
- `for await` reads as prose and reuses `for … in`, so there is one loop form for collections and streams.
- An explicit process-wide scope makes shared server state rare and visible. Everything else stays isolated per request by default.
- Keyed broadcasts filter where the data is, so memory caps and privacy hold per subscriber.

## Open questions

- Whether the client runtime should offer automatic re-subscription with backoff, instead of every caller writing its own.
