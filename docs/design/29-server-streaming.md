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
- **Shutdown ends every open stream at once.** A stream never finishes by itself, so waiting for it would hold every shutdown (and every `k dev` restart) for the whole grace period (`K_SHUTDOWN_GRACE`, 25 s). When the server stops accepting, it ends each open stream's response; the client sees a finished stream, as after a dropped connection, and subscribes again, to another replica or to the restarted server. Ordinary requests in flight still get the grace period.
- **A stream ends when its session ends.** The runtime closes every stream opened under a session on sign-out, expiry or `endAll` ([26-auth-and-sessions.md](26-auth-and-sessions.md#lifetime-and-revocation)), so a signed-out tab stops receiving data even though guards ran only when it opened.

## `Stream<T>`

`Stream<T>` is a core type: an asynchronous sequence that is read once.

| Member | Meaning |
|---|---|
| `filter { … }`, `map { … }` | lazily transformed streams; the function may be `async` |
| `prefix(while: { … })` | the elements up to the first the function refuses, then the end; the function may be `async` |
| `prefix(through: { … })` | the elements up to and including the first the function accepts, then the end; the function may be `async` |
| `startingWith(value)` | a stream that yields `value` first, then the elements of this one |
| `Stream.merge(a, b)` | the elements of both as they arrive ([44-channels.md](44-channels.md)) |
| `next() async -> T?` | the next element, or `nil` once the stream has ended |

### Stream operators

- **`filter`, `map` and `prefix(while:)` take an async function, `(T) async -> Bool` or `(T) async -> U`.** A lambda passed to them may `await`, so a channel's stream can check each event against the database (`.filter { event => await memberships.isMember(room: event.room, member: me) }`). A synchronous function or lambda is accepted as it is, since a function that never suspends is a valid async one.
- **One element at a time, in order.** The function runs for an element only when the reader asks for the next one, and the next element waits until it returns, so a stream never reorders and never runs two calls at once. Nothing reads ahead of the reader.
- **`prefix(while:)` ends the stream at the first element its function refuses,** dropping that element, and closes the stream it reads, so a `Broadcast` subscription behind it is released at once rather than when the reader goes away.
- **`prefix(through:)` ends it after the first element its function accepts, yielding that element,** and closes what it reads the same way. It is how a server ends one channel with a last message the client reads as the reason ([44-channels.md](44-channels.md#ending-one-channel-from-the-server)): `prefix(while:)` would drop exactly the element that says why. It is one more step result of the operators' shared runtime class, not a class of its own.
- **Closing a derived stream closes what it reads, and cancels the call running.** Leaving a `for await` loop, a channel closing, or the end of a request closes the outermost stream; the close reaches every stream it was derived from, and a `filter` or `map` function suspended at an `await` never resumes, like any cancelled async work ([03-reactivity.md](03-reactivity.md#async-work-is-owned)).
- The functions don't throw: a stream has nowhere to deliver a typed error, and a failure inside one is a panic, which ends a server stream as a failure ([§ Reading a stream](#reading-a-stream-for-await)).

Why: an event's relevance often depends on stored state (membership, permissions, a moderator's removal on another replica), and with only synchronous operators a handler mirrored that state in per-channel fields that went stale. Swift's `AsyncSequence` takes async closures in the same three operators for the same reason. **Alternatives:** separate `filterAsync`/`mapAsync` names (two spellings of one idea, and the sync ones would be redundant, since a sync function is a valid async one); concurrent mapping with a limit (reorders or buffers, and costs runtime size for a need no example has); a throwing variant whose error ends the stream (a reader can't tell an error from the end, and the channel's typed refusal is the place for errors). The three operators share one runtime class with `startingWith` in the stdlib bodies (`std.js`), which grew by 77 B gzip (9,113 B to 9,190 B of 10,240 B); the full client runtime is unchanged, and the shipped runtime chunk of the apps with streams grew about 30 B (huddle 10,108 B to 10,140 B).

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
