# Channels: Typed Realtime Between Browser and Server

## Decision

A **channel** is a long-lived, two-way, typed conversation between one browser tab and the app's server. It is declared as an **rpc in a protocol whose result is `Channel<C, S, E>`**: the client sends messages of type `C`, the server sends messages of type `S`, and the server may refuse a client message with the error enum `E`. One WebSocket per tab carries every channel the tab opens. The runtime owns the hard parts: connecting, heartbeats, reconnecting with backoff, resuming without loss after a short drop, size limits, back-pressure, and closing everything when the session ends.

```k
// chat/ChatProtocol.k: the contract
public protocol ChatProtocol requires Authenticated {
    /// Messages of a room, newest last, for scrolling back.
    rpc history(room: RoomId, before: MessageId?) throws RoomError -> List<ChatMessage>

    /// Joins a room until the caller leaves it: a snapshot first, then every event.
    rpc room(id: RoomId) throws RoomError -> Channel<ChatCommand, ChatEvent, CommandError>
}

public enum ChatCommand implements Codable {
    case post(text: MessageText, clientId: Uuid)
    case edit(id: MessageId, text: MessageText)
    case delete(id: MessageId)
    case typing
}

public enum ChatEvent implements Codable, Equatable {
    case snapshot(messages: List<ChatMessage>, present: List<Member>)
    case posted(ChatMessage, clientId: Uuid)
    case edited(ChatMessage)
    case deleted(MessageId)
    case presence(PresenceChange<Member>)
    case typing(AccountId)
}

public enum CommandError implements Error {
    case notYourMessage
    case messageGone
    case rpcFailure(RpcFailure)
}
```

```k
// chat/ChatApi.k: the server side
public service ChatApi implements ChatProtocol {
    @Inject let session: Session
    @Inject let messages: MessageRepository
    @Inject let rooms: RoomEvents          // @Scope(.server): a Broadcast<RoomId, ChatEvent>
    @Inject let presence: RoomPresence     // @Scope(.server): a Presence<RoomId, Member>

    rpc room(id: RoomId) async throws RoomError -> Channel<ChatCommand, ChatEvent, CommandError> {
        guard await messages.canRead(room: id, user: session.user.id) else {
            throw RoomError.notAMember
        }

        let live = Stream.merge(                                  // subscribe first, so nothing is missed
            rooms.subscribe(room: id),
            presence.watch(room: id).map { change => ChatEvent.presence(change) }
        )
        presence.join(room: id, member: Member(of: session.user)) // left when this channel closes
        let recent = await messages.recent(room: id, limit: 50)
        let present = await presence.members(room: id)

        return Channel(sending: live.startingWith(.snapshot(messages: recent, present: present))) { command =>
            try await handle(command: command, room: id)
        }
    }

    // Runs once per client message, in order, one at a time per channel.
    private func handle(command: ChatCommand, room: RoomId) async throws CommandError {
        match command {
            case .post(let text, let clientId) {
                let message = await messages.post(room: room, author: session.user.id, text: text)
                rooms.publish(room: room, event: .posted(message, clientId: clientId))
            }

            case .edit(let id, let text) {
                guard let message = await messages.find(id: id) else { throw CommandError.messageGone }
                guard message.author == session.user.id else { throw CommandError.notYourMessage }

                rooms.publish(room: room, event: .edited(await messages.edit(message, text: text)))
            }

            case .delete(let id) { … }

            case .typing {
                rooms.publish(room: room, event: .typing(session.user.id))
            }
        }
    }
}
```

```k
// chat/RoomService.k: the client side
public service RoomService {
    @Inject let chat: ChatProtocol

    @State var messages: List<ChatMessage> = []
    @State var present: List<Member> = []
    @State var channel: Channel<ChatCommand, ChatEvent, CommandError>? = nil

    /// Reconnecting, offline or open: what a banner shows.
    @Derived let connection: ChannelState = channel?.state ?? .connecting

    /// Follows a room until the caller is cancelled (the page is left) or the server closes it.
    func follow(room: RoomId) async throws RoomError {
        let opened = try await chat.room(id: room)
        channel = opened

        for await event in opened.messages {
            apply(event: event)
        }
    }

    func post(text: MessageText) async throws CommandError {
        guard let channel else { throw CommandError.rpcFailure(.offline) }

        try await channel.send(.post(text: text, clientId: Uuid.random()))
    }

    func apply(event: ChatEvent) { … }
}
```

A page starts `follow` from an `@Effect(on: [roomId])`, exactly as it follows a server stream ([29-server-streaming.md](29-server-streaming.md#following-a-stream-for-the-life-of-a-page)): leaving the page cancels the effect, which closes the channel, which leaves the room's presence on the server.

## Why a channel is an rpc

**A channel is opened by an rpc call, so everything Kobra already guarantees for rpcs holds at the moment it opens:** the wire name, `rpc.lock`, the protocol's and the rpc's `requires` guards, `Validatable` parameters, typed errors with `rpcFailure`, `session.user` narrowing, the server/client boundary, and injection of the protocol into services only. The only new thing in the signature is the result type, as `Stream<T>` was for server streaming.

| Result type | Kind | Transport |
|---|---|---|
| `T` | unary rpc | one HTTP request ([28-rpc-wire-format.md](28-rpc-wire-format.md)) |
| `Stream<T>` | server stream | one long HTTP response ([29-server-streaming.md](29-server-streaming.md)) |
| `Channel<C, S, E>` | channel | the tab's WebSocket, multiplexed (this file) |

### `Channel<C, S, E>`

- **`C`** (client to server) and **`S`** (server to client) are `Codable`. They are usually enums, one case per kind of message, so both sides `match` exhaustively.
- **`E`** is the error enum a client message can be refused with. Like every rpc error enum it has the case `rpcFailure(RpcFailure)` ([28-rpc-wire-format.md](28-rpc-wire-format.md#failures-outside-the-handler)); a channel with no refusals of its own uses `RpcOnlyError`.
- **The rpc's own `throws` clause is for opening:** `RoomError` above is what joining can fail with (`notAMember`, a guard's refusal, `offline` on the first connect). `E` is what each message can fail with (`notYourMessage`). The two are separate because they are different questions, and a client handles them in different places.
- A `Channel` can be written as a type anywhere on the client (a service field, as above). On the server it is only built, by the handler, and returned.
- **Rules (codes as built: K5503, K5502, K5509, K5510; see "Decided while implementing"):** a `Channel` result outside a protocol rpc or its handler is an error; so is a `Channel` inside another type (`List<Channel<…>>`, `Stream<Channel<…>>`), a non-`Codable` `C` or `S`, an `E` without `rpcFailure`, and a `Secret` or other server-only type inside `C` or `S` (as for error payloads, [28-rpc-wire-format.md](28-rpc-wire-format.md#errors-and-status-codes)).

## The server: one handler run per channel

- **The handler runs once, when the channel opens,** after the guards, like a streaming handler. It may `await` to load a snapshot, and it returns a `Channel` built from two things:
  - **`sending: Stream<S>`**, every message the server will send on this channel, usually a `Broadcast` subscription started with a snapshot. When this stream ends, the server has closed the channel (`ChannelClose.byServer`), and the client doesn't reconnect.
  - **a receiving function `(C) async throws E`**, the trailing lambda, which runs for each client message.
- **Throwing from the handler refuses the open**, as for any rpc: the client's `try await chat.room(id:)` throws it. Throwing `E` from the receiving function refuses that one message: the client's `send` throws it, and the channel stays open.
- **A channel's request lasts as long as the channel.** The request container the handler ran in, with the implementing service's instance, its `@Scope(.request)` and `.app` services and its `Session`, stays alive until the channel closes. **Per-connection state is therefore ordinary state:** a private `var` of the implementing service, or a request-scoped service it injects, is one per open channel and needs no locks, because:
- **Messages of one channel are handled in order, one at a time.** The next message waits until the receiving function returns. Different channels, including several of one tab, run concurrently. An app that wants a slow command not to hold up the channel hands it to a background job ([30-background-jobs.md](30-background-jobs.md)).
- **The server acknowledges each message when the receiving function returns** (or throws, carrying the error), after any `database.transaction` in it has committed. A `Broadcast` published inside that transaction is delivered on commit ([29-server-streaming.md](29-server-streaming.md#sharing-events-between-requests)), so the sender may see its own echo before or after its acknowledgement. Apps apply events idempotently, as `NoteChange` already does, and match an echo to a pending message by a client id.
- **Validation:** a `Validatable` `C` (or a message whose payload is) is validated before the receiving function runs; a failure refuses the message with `rpcFailure(.invalid(…))`, which a `@Form` shows under its fields ([15-forms-and-validation.md](15-forms-and-validation.md)).
- **`Channel(sending:closed:receiving:)` has an optional `closed: (ChannelClose) -> Void`** for cleanup on a normal close. It is best-effort, since a crashed process runs nothing; anything that must be undone after a crash (presence, below) uses a lease instead.

### Ending one channel from the server

**A handler ends its channel by ending its `sending` stream, and says why in the last message it sends** (decided 2026-11-11, from huddle's member removal). The stream's end is the close (`ChannelClose.byServer`, after every message before it has been sent), so the client's `messages` loop reads the reason as an ordinary, typed `S` and then ends, and `state` is `.closed(.byServer)`. `Stream.prefix(through:)` ([29](29-server-streaming.md#stream-operators)) is the operator for it: it yields the first element its function accepts and then ends, so the element that ends the channel is delivered.

```k
// examples/huddle/src/chat/ChatApi.k: the person's own removal is the last message
let live = Stream.merge(
    events.subscribe(room: name),
    presence.watch(room: name).map { change => ChatEvent.presence(change) }
).prefix(through: { event => event == .removed(name: me) })
```

- **The decision usually comes from somewhere else:** a moderator's command handled by another channel, often on another replica. That channel can't name this one, but it can publish to a `Broadcast` this one's stream reads, which already crosses replicas. So the stream is where the decision arrives, and ending it is where it acts.
- **Ending the stream ends the channel's request:** its presence leases are left, the `closed` function runs with `.byServer`, and a message that arrives afterwards is refused `rpcFailure(.offline)` on the client (the channel is closed). A message already on its way when the decision was published still reaches the receiving function, which checks the database when it matters (huddle's commands do).
- **K5511 stays:** server code still reads no `Channel` member; `close()` is the browser's.

Alternatives rejected: **a server-side `close(reason:)` on the handler's `Channel`** (a new member that K5511 would have to allow on one side only, and it would still need the broadcast to learn the decision from another replica); **a `ChannelClose.byServer(reason)` case carrying an app value** (`ChannelClose` would become generic over `S`, and so would `ChannelState`, for a reason that is already an `S`); **a `closing: (S) -> Bool` parameter of `Channel(sending:…)`** (an operator any stream can use says the same without a channel-only rule).

### Rooms, topics and broadcast

**Channels add no fan-out mechanism of their own: rooms and topics are `Broadcast` keys** ([29-server-streaming.md](29-server-streaming.md#sharing-events-between-requests)). A room is `Broadcast<RoomId, ChatEvent>`, "every tab of this user" is `Broadcast<AccountId, AccountEvent>`, and a cursor overlay on one board is `Broadcast<BoardId, CursorMove>`. A handler can merge several into its `sending` stream with `Stream.merge(_:_:)`, a new member of `Stream<T>` that yields elements of both as they arrive and ends when both have.

- **One channel per subject.** A tab that switches rooms closes one channel and opens another, rather than subscribing and unsubscribing inside a channel. Channels are cheap (a frame to open, multiplexed over one socket), and this keeps each channel's authorization a single open-time decision plus per-message checks.
- **The sender gets its own echo.** `Broadcast` has no "everyone except me", because that needs a subscriber identity and invites gaps; an echo applied idempotently costs one message.
- **Ordering:** messages on one channel arrive in the order the server sent them. A `Broadcast` keeps the order of one publisher's publishes for a key; there is no total order across publishers or replicas. An app that needs one (chat messages) orders by a database sequence it assigns, and the client sorts.

### Per-message authorization and limits

- **Guards run when the channel opens.** `requires Authenticated` and `Role` hold for the channel's life because ending the session closes it ([below](#sessions-and-security)), and a lost role takes effect with `endAll` as for rpcs ([26-auth-and-sessions.md](26-auth-and-sessions.md#lifetime-and-revocation)). `RateLimit` on a channel rpc limits opens.
- **Authorization of each message is the receiving function's**, which reads `session.user`, narrowed there as in the handler (the lambda is part of the guarded body), and throws `E` to refuse: `notYourMessage` above, or `CommandError.forbidden` after checking `session.user.roles`.
- **Every channel has a message rate limit**, counted per channel in memory (no database row per message): by default 20 messages a second sustained, with bursts of 50. A message over it is refused with `rpcFailure(.rejected(.rateLimited(retryAfter:)))` without running the receiving function.

## The client

`try await chat.room(id:)` opens the channel and returns a `Channel<C, S, E>`:

| Member | Meaning |
|---|---|
| `state: ChannelState` | reactive: `connecting`, `open`, `reconnecting(attempt:at:)`, `closed(ChannelClose)` |
| `messages: Stream<S>` | every server message, in order, across reconnects; ends when the channel closes for good. Read once. |
| `send(_ message: C) async throws E` | sends and waits for the server's acknowledgement; throws the server's refusal, or `rpcFailure`. On a channel that is `.closed`, it throws `rpcFailure(.offline)` at once |
| `close()` | closes the channel for good; idempotent |

```k
public enum ChannelState implements Equatable {
    case connecting                            // the first open, or a reopen after the socket came back
    case open
    case reconnecting(attempt: Int, at: Date)  // the next attempt is due at `at`
    case closed(ChannelClose)
}

public enum ChannelClose implements Equatable {
    case byClient       // close(), or the owner that opened it was cancelled
    case byServer       // the handler's sending stream ended
    case sessionEnded   // sign-out, expiry or endAll; the router reacts as for any rpc
    case refused        // a restart was refused: a guard, or the handler threw
}
```

- **The first open behaves like any rpc call:** guards, the handler's errors, `rpcFailure(.offline)` and `.timeout` are thrown by the call. The runtime doesn't retry the first open, because the caller is already waiting for an answer and can show one.
- **After the first open, the runtime owns reconnection.** `for await` keeps going across drops; the page shows `connection` and does nothing else.
- **The channel is owned by the run that opened it** ([03-reactivity.md](03-reactivity.md#async-work-is-owned)): when the effect is cancelled or re-runs, or the page is left, the channel is closed (`byClient`) and the loop stops silently.
- **Opening a channel during server rendering panics** ("channels open only in the browser: open it from an `@Effect`"). Effects don't run on the server, so code that follows the documented pattern never hits it; a call reachable from a `@Load` is a compile warning where the checker can see it.

### Reconnecting and resuming

The socket can drop at any time: a laptop lid, a train tunnel, a deploy. The runtime notices within one heartbeat ([below](#heartbeats)) and then:

1. **Backs off with full jitter:** a random delay up to 0.5 s, 1 s, 2 s, 4 s, 8 s, 16 s, then up to 30 s for every later attempt, forever while the owner lives. It retries at once when `Connectivity.isOnline` turns `true` or `PageVisibility.isVisible` does ([45-web-platform.md](45-web-platform.md#connectivity)), and doesn't try while `isOnline` is `false`. A failed reconnect sets `isOnline` to `false` as a failed rpc does, so the offline banner and the channel agree. The delays are on the injected `Clock`, so `TestClock` drives them ([35-time.md](35-time.md)), and in `.test` the jitter draws from the test's seeded random source ([33-crypto.md](33-crypto.md#testing)), so a test's timings repeat exactly.
2. **Resumes when it can.** Every channel has a resume token, and every message in each direction a sequence number. The client confirms what it has received (a `rcvd` frame with the highest sequence number, at most once a second while messages arrive, and with each heartbeat reply), and the server keeps a message in the channel's outgoing buffer only until it is confirmed. Within the **resume window** (30 s) after a drop, the server keeps the channel's request container, subscription and outgoing buffer. A reconnect that presents the token (and the same session) reattaches to them: the server replays what the client hadn't received, the client re-sends what the server hadn't acknowledged, and the server skips any it had already handled. **Nothing is lost or handled twice, and the handler doesn't run again.**
3. **Restarts when it can't.** After the window, after a deploy, on another replica, or after the outgoing buffer overflowed, the token is unknown, so the runtime opens the channel again with the same arguments. **The guards and the handler run again,** under the current session, so a protocol whose handler starts with a snapshot, as every example does, brings the client up to date with nothing else to write. A refusal now (the user was removed from the room) closes the channel with `.refused`. The refusal is of the rpc's open error type, which `Channel<C, S, E>` doesn't carry, so the channel doesn't hold it: an app that wants to say why calls the rpc again, and that call throws the typed error (`RoomError.notAMember`). A refused restart is rare, and one extra rpc is cheaper than a fourth type parameter.

**Messages sent while reconnecting** wait, at most 10 seconds and at most 32 per channel:

| A message that… | on resume | on restart |
|---|---|---|
| was never transmitted | sent | sent on the new channel |
| was transmitted, not acknowledged | re-sent; handled at most once | **`send` throws `rpcFailure(.offline)`**: the old server may or may not have handled it, so the caller decides, as for an rpc ([28-rpc-wire-format.md](28-rpc-wire-format.md#network-failures)) |
| waited longer than 10 s | `send` throws `rpcFailure(.offline)` | the same |

- **There is no persistent offline queue.** Messages live in the tab's memory; a draft that must survive a reload is the app's, in `LocalStorage` ([36-local-storage.md](36-local-storage.md)). A queue that silently replays commands minutes later, after the world changed, is a bug factory; a 10-second bound covers a blip and makes a real outage visible.
- **A cancelled `send`** (its owner unmounted) withdraws the message if it hasn't been transmitted, and otherwise ignores the acknowledgement.
- **The back-forward cache.** Browsers don't keep a page with an open WebSocket in the back-forward cache, so on `pagehide` the runtime closes the socket as a drop (the server starts the resume window) and on `pageshow` reconnects, resuming if the page came back within 30 seconds. Going back to a chat is then instant and lossless.

### Delivery guarantees

| | client → server (`C`) | server → client (`S`) |
|---|---|---|
| order | the order `send` was called, per channel | the order the server sent, per channel |
| while connected, and across a resume | exactly once, acknowledged | exactly once |
| across a restart | at most once: unacknowledged messages throw `.offline` | messages sent between the drop and the restart are lost; the handler's new snapshot replaces them |
| across channels, tabs or replicas | no order | no order (an app that needs one sorts by a sequence it assigns) |

The one place a message can be lost is the restart, and that is exactly where the protocol's snapshot runs. A handler that sends no snapshot accepts that gap; the book's channel chapter starts every example with one.

### Heartbeats

- **The server sends a heartbeat frame every 25 seconds on every socket,** from the Rust front, without waking JavaScript. That keeps proxies with a 60-second idle timeout (nginx's default) from cutting idle sockets.
- **The client treats a socket that delivered nothing for 60 seconds as dead** and reconnects. Browsers don't expose WebSocket pings, so the heartbeat is a frame, not a ping. The server closes a socket that sent nothing (not even its own heartbeat reply) for 60 seconds, which starts the resume window.

## Sessions and security

- **The socket is opened with `GET /_k/channels`** and the `Upgrade: websocket` header, which carries the session cookie. The path is reserved: a page or endpoint page at it is a route conflict.
- **The upgrade requires an `Origin` matching `K_PUBLIC_ORIGIN`,** or is refused with `403` before anything else runs. Browsers send cookies with cross-site WebSocket handshakes in more cases than `SameSite=Lax` suggests, and CORS doesn't apply to them, so this check is the defence against cross-site WebSocket hijacking. It extends the forgery rules in [26-auth-and-sessions.md](26-auth-and-sessions.md#request-forgery), which exempt `GET`.
- **The subprotocol is `k.channel.v1`;** a handshake without it is refused with `400`.
- **Ending a session closes its channels** on every replica, exactly as it closes its streams ([29-server-streaming.md](29-server-streaming.md#the-handler)): they close with `.sessionEnded`, running receive functions stop at their next `await`, and the socket delivers the guest mirror, so the router disposes a guarded page at once instead of at the next rpc ([26-auth-and-sessions.md](26-auth-and-sessions.md#in-the-browser)).
- **A change of session in the browser replaces the socket.** After a sign-in, a sign-out or a mirror change, the client closes its socket and opens a new one with the new cookie; channels whose owners are still alive restart on it, so no channel ever runs under a session other than the one it was opened with.
- **Sending a message slides the session's idle timeout,** as an rpc does; heartbeats don't, so an idle tab doesn't keep a session alive for ever. The absolute lifetime ends the session, and with it the channels, as usual.
- **Limits per session:** at most 16 sockets (tabs) and 32 channels per socket. An open over either limit is refused with `rpcFailure(.rejected(.rateLimited(retryAfter:)))`.

## Limits and back-pressure

| Limit | Default | When it's exceeded |
|---|---|---|
| encoded message size, each direction | 64 KiB | client: `send` throws `rpcFailure(.invalid(…))` before transmitting; server: the socket is closed (a Kobra client never sends one) |
| unacknowledged messages a client has in flight, per channel | 32 | `send` waits for an acknowledgement |
| messages waiting to be sent while reconnecting | 32 per channel, 10 s | `send` throws `rpcFailure(.offline)` |
| messages per second, per channel | 20, bursts of 50 | the message is refused with `.rateLimited` |
| server's outgoing buffer: messages sent and not yet confirmed received, per channel | 1,000 messages or 4 MB | the channel is dropped without a resume token, and the client restarts it (a fresh snapshot) after its usual backoff, so a client that can't keep up doesn't restart in a tight loop |
| resume window | 30 s | the next reconnect restarts |
| heartbeat; silence before a socket is dead | 25 s; 60 s | reconnect |

- **The settings are the stdlib's `ChannelConfig`** (`K_CHANNEL_MAX_MESSAGE`, `K_CHANNEL_RESUME_WINDOW`, `K_CHANNEL_HEARTBEAT`, `K_CHANNEL_RATE`), read like every stdlib config ([25-config-and-secrets.md](25-config-and-secrets.md#stdlib-configs)), so `dist/app config` lists them.
- **A slow reader never slows anyone else.** A `Broadcast` publish never blocks ([29-server-streaming.md](29-server-streaming.md#sharing-events-between-requests)); the slow channel's own buffer fills and that channel restarts. The 64 KiB size limit is checked in Rust before decoding, like the 1 MB rpc body limit.
- **Large payloads don't belong in a channel.** A file goes through an rpc or an endpoint page; the channel carries the event that it changed.

## Server isolates and replicas

`k_server` runs one V8 isolate per core, sharing nothing ([docs/compiler/server.md](../compiler/server.md)). Channels fit that model instead of fighting it:

- **The Rust front accepts the socket, checks `Origin` and the subprotocol, sends heartbeats, enforces sizes and buffers, and pins the socket to one worker isolate for its life** (the one with the fewest open channels). Every channel of that socket opens, runs and keeps its request container in that isolate. Frames reach JavaScript through ops (`op_k_channel_next`, `_send`, `_ack`, `_close`).
- **Per-connection state never crosses isolates;** it doesn't need to, since a connection lives in one.
- **Fan-out does cross them, through `Broadcast`,** which already lives in the process's Rust state and carries elements in their codec's encoding ([docs/compiler/codegen.md](../compiler/codegen.md)). A command handled on isolate 3 reaches subscribers on isolates 0–7 without any isolate knowing about the others.
- **Resume is in-process.** The resume token names the process, the worker and the socket (with 128 random bits, bound to the session id). A reconnect that lands on the same process reattaches through the front to the right isolate. One that lands on another replica can't, and restarts, which is correct, only less efficient. **Sticky load balancing is optional:** it improves how often a reconnect resumes, but nothing depends on it.
- **Between replicas, `Broadcast` goes through its `BroadcastProvider`** ([29-server-streaming.md](29-server-streaming.md#sharing-events-between-requests)). The defaults, with no configuration and no third-party account:

| The app's database | Fan-out |
|---|---|
| none | in-process (one replica) |
| SQLite | in-process (SQLite is a one-replica database) |
| Postgres | `LISTEN`/`NOTIFY` on one channel per app. A payload over 7,000 bytes (under `NOTIFY`'s 8,000-byte limit) is written to `k_broadcast_spill` and the notification carries its id; rows older than a minute are deleted. |
| anything else | a `BroadcastProvider` from a git package (Redis, NATS), bound with `use` in `App.k` |

- **On `SIGTERM`, a replica stops accepting sockets and closes every open one with "going away" and a reconnect delay spread over 0–5 seconds,** so a rolling deploy doesn't reconnect every tab in the same instant. The channels restart on another replica. Shutdown waits for running receive functions within `shutdownGrace` ([38-deployment.md](38-deployment.md#startup-and-shutdown)), not for the sockets.
- **Proxies must pass `Upgrade`.** The deployment docs gain the nginx lines (`proxy_http_version 1.1`, `proxy_set_header Upgrade $http_upgrade`, `Connection "upgrade"`, `proxy_read_timeout 75s`), and `examples/huddle` ships them. WebSockets over HTTP/2 (RFC 8441) aren't needed, since one socket per tab is far from any connection limit.

## Presence: a worked example

Who is in the room, and who is typing, is the most-requested realtime feature and the easiest to get wrong: a crashed replica leaves ghosts, two tabs of one user flicker, and a disconnect without a goodbye leaves someone "online" forever. **`Presence<Key, Member>`** in `k.rpc` does it once, on top of `Broadcast`:

```k
// chat/RoomPresence.k
@Scope(.server)
public service RoomPresence {
    private let presence: Presence<RoomId, Member> = Presence()

    /// Adds the member to the room until the channel (the request) that called this closes.
    func join(room: RoomId, member: Member) -> PresenceLease<Member> {
        return presence.join(key: room, member: member)
    }

    func members(room: RoomId) async -> List<Member> {
        return await presence.members(key: room)
    }

    func watch(room: RoomId) -> Stream<PresenceChange<Member>> {
        return presence.watch(key: room)
    }
}

public struct Member implements Codable, Hashable {
    let id: AccountId
    let name: String
}
```

| Member | Meaning |
|---|---|
| `join(key:member:) -> PresenceLease<Member>` | adds `member` under `key` for as long as the calling request lives; for a channel, until it closes |
| `members(key:) async -> List<Member>` | everyone present, each `Member` once, in the order they arrived |
| `watch(key:) -> Stream<PresenceChange<Member>>` | `.joined(m)`, `.left(m)`, `.updated(m)` from now on, delivered through the key's `Broadcast` |
| `lease.update(member:)` | replaces this lease's member (a status such as "away") and announces `.updated` |
| `lease.leave()` | leaves early; idempotent |

- **A lease belongs to the request that took it,** so the handler above writes no cleanup: when the channel closes, normally, by a dropped socket that wasn't resumed, or by the session ending, the member leaves. A resumed channel never left, so a blip doesn't flicker.
- **Members are counted, not listed.** Two tabs of the same user hold two leases with equal `Member` values; the room sees one `joined` when the first arrives and one `left` when the last goes.
- **A crashed replica can't leave ghosts.** Each process renews its leases every 10 seconds; a lease not renewed for 30 seconds expires on every replica. Without Postgres, leases are in process memory; with it, in `k_presence` (key, member, process, expiry), and changes are announced over `NOTIFY`.
- **"Typing…" isn't presence.** It is an ordinary event (`ChatEvent.typing(AccountId)`) that the client shows for 5 seconds after the last one, on its `Clock`. Nothing stores it, so nothing has to clean it up.

The handler at the top of this file merges `presence.watch(room: id)` into its `sending` stream with `Stream.merge`, mapped to `ChatEvent.presence`, after subscribing and before reading `members`, so no change between the two is missed.

## Choosing: rpc, stream or channel

| Need | Use |
|---|---|
| ask once, get an answer (load, save, search) | an **rpc** |
| follow changes the server makes; the client only listens (a live list, a dashboard, notifications) | a **server stream** (`Stream<T>`) |
| the client also sends often, small and fast (chat, cursors, typing, game input, collaborative editing), or the server needs to know who is connected right now | a **channel** |

- **Streams stay on HTTP.** They cross every proxy, share one connection across tabs ([29-server-streaming.md](29-server-streaming.md#one-connection-per-stream-shared-across-tabs)) and need no socket. A channel adds per-connection state, acknowledgements and presence; it isn't a better stream.
- **Request/response stays an rpc,** even next to an open channel. `history(room:before:)` above is an rpc because it answers one caller. A channel message has no return value: what the server answers goes on the channel to everyone who should see it, or comes back as the refusal `E`.

## Raw WebSockets: `k.http`

Third-party realtime endpoints (an exchange's price feed, a self-hosted chat bridge, a local device) speak their own protocol, so they get a plain, typed client next to `HttpClient` ([32-http-client.md](32-http-client.md)):

```k
public interface WebSocketClient {
    func connect(request: WebSocketRequest) async throws WebSocketError -> WebSocket
}
```

| `WebSocket` member | Meaning |
|---|---|
| `state: WebSocketState` | reactive: `open`, `closing`, `closed(code: Int, reason: String)` |
| `messages: Stream<WebSocketMessage>` | `.text(String)` and `.binary(Bytes)`; ends when the socket closes. `message.json()` decodes a `Codable` |
| `send(_ message: WebSocketMessage) async throws WebSocketError` | waits while the browser's buffer is over 1 MB |
| `close(code:reason:)` | idempotent |

- **`WebSocketRequest` is a `@Builder` struct like `HttpRequest`:** `WebSocketRequest(url: Url("wss://feed.example/v1")).subprotocol("feed.v2")`. **`.header(_:_:)` is server-only,** because a browser can't send headers on a handshake, so a token in a header (a `Secret`) can't reach client code; using it on the client is a compile error.
- **`WebSocketError`:** `offline`, `timeout`, `refused(status: HttpStatus)` (the handshake was answered without upgrading), `tooLarge` (an incoming message over `maxMessage`, default 1 MB), `closed`.
- **Both sides:** in the browser it wraps `WebSocket`; on the server it is a Rust op, so a service can bridge a third-party feed into a `Broadcast` that browser channels subscribe to, keeping the feed's credentials server-only.
- **No automatic reconnection.** The runtime can't know how to resubscribe to someone else's protocol. `k.time` gains a `Backoff` value with the channel's policy (`backoff.next() -> Duration`, `reset()`), so a hand-written loop is three lines and uses the same delays.
- **Owned like an `HttpClient` call:** a socket opened by a run is closed when its owner is cancelled.
- It is injectable only into services, like `HttpClient`, and has one production implementation, so it needs no `use` rule.

## `rpc.lock` and compatibility

Open tabs are deployed clients that may live for days, so a channel's messages are recorded like every other wire shape ([40-workspaces-and-services.md](40-workspaces-and-services.md#compatibility-between-deploys)):

- **A channel rpc is recorded with `kind = "channel"`** and its three types (`send`, `receive`, `refusal`) beside its parameters and open error. Its wire name is an rpc's (`/huddle.chat.ChatProtocol/Room`); the open frame names it.
- **Breaking-change detection knows the direction,** because the two sides upgrade at different times:

| Change | `C` (client → server) | `S` (server → client) |
|---|---|---|
| add a case, an optional field, or a field with a default ([08-types.md](08-types.md#enums-with-associated-data)) | compatible: the field decodes to `nil` or its default when an old tab leaves it out | compatible: an old tab skips a case it doesn't know, and the dev build logs it; it ignores a field it doesn't know |
| remove a case | **breaking:** old tabs still send it | compatible: the server stops sending it |
| add a required field | **breaking** | **breaking** |
| change a field's type, reuse a number | **breaking** | **breaking** |

- A type used in both directions, or also by an rpc, gets the stricter rule of each place it appears. Turning a channel into an rpc or a stream, or back, is breaking. `@Breaking("reason")` allows any of them, as for rpcs.
- **Skipping an unknown `S` case is the only silent tolerance on the wire,** and it is safe only because a channel's state is rebuilt from snapshots: the old tab misses a feature until it reloads, never a correction. The runtime counts skipped cases in `k_channel_unknown_total{method}`: the browser reports them with its next `rcvd` (below, "The unknown-case metric").

## The wire

- **Frames are binary Protobuf in production and JSON text in development,** as rpcs are ([11-protocols-and-rpc.md](11-protocols-and-rpc.md)), so the browser's devtools show readable frames under the socket. Messages use the rpc JSON mapping ([28-rpc-wire-format.md](28-rpc-wire-format.md#messages)).
- **A non-K client can speak it:** `k proto export` emits the frame messages of `k.channel.v1` beside the app's protocols.

```text
→ {"open":   {"ch": 1, "method": "/huddle.chat.ChatProtocol/Room", "request": {"id": "…"}}}
← {"opened": {"ch": 1, "resume": "…"}}                 (or {"refused": {"ch": 1, "error": {…}}})
← {"msg":    {"ch": 1, "seq": 1, "value": {"snapshot": {…}}}}
→ {"msg":    {"ch": 1, "seq": 1, "value": {"post": {"text": "hi", "clientId": "…"}}}}
← {"ack":    {"ch": 1, "seq": 1}}                       (or with "error": the refusal E)
→ {"rcvd":   {"ch": 1, "seq": 14}}                      (received up to 14; the server trims its buffer)
→ {"resume": {"ch": 1, "token": "…", "received": 14, "sent": 3}}
← {"close":  {"ch": 1, "reason": "byServer"}}
← {"session":{"user": null}}                            (the guest mirror, after the session ended)
↔ {"beat":   {}}
```

## Testing

Every part runs in `k test` without a browser, a socket or a second process. Channels in tests run **in-process through the real framing**, as rpcs run through the real wire format ([21-testing.md](21-testing.md#which-side-a-test-runs-on)): messages are encoded and decoded, guards and validation run, acknowledgements and refusals arrive typed, and the size and rate limits apply.

### `TestChannels`: the test's connection

`@Inject let network: TestChannels` (the default in `.test`; no `use` rule) controls the in-process connection between the test's browser and server. Going offline is not here: `TestConnectivity.goOffline()` ([45-web-platform.md](45-web-platform.md#connectivity)) is the one switch, and it drops the socket and stops reconnects as well as failing rpcs, until `goOnline()`.

| Member | Meaning |
|---|---|
| `drop()` | the socket drops; channels go `reconnecting` and resume after their backoff, on the `TestClock` |
| `drop(resumable: false)` | the socket drops and the server forgets it, as after a deploy: channels restart, and their handlers run again |
| `open: List<OpenChannel>` | the channels open now, with their method and arguments |

```k
test RoomPage {
    @Inject let sessions: SessionManager
    @Inject let network: TestChannels
    @Inject let clock: TestClock

    test "a dropped connection shows a banner and recovers without losing messages" {
        await sessions.signIn(userId: AccountId.ana)
        let screen = render(RoomPage(roomId: RoomId.general))
        await screen.settle()

        network.drop()
        await screen.settle()
        expect screen.status().text == "Reconnecting…"

        clock.advance(by: 1.second)
        await screen.settle()
        expect not screen.status().exists
    }
}
```

### `TestPeer`: a second browser

Realtime features are about other people, so **a test can open more browsers**. `@Inject let peers: TestPeers` and `let bob = await peers.signIn(userId: AccountId.bob)` give a `TestPeer`: a browser container with its own session, sharing the test's server container, database and clock. `bob.render(…)` mounts a view in it, and `let chat: ChatProtocol = bob.client()` gives its protocol client (the protocol comes from the expected type, as for `RemoteClient.at`).

```k
test "a message posted by one person appears for another" {
    await sessions.signIn(userId: AccountId.ana)
    let ana = render(RoomPage(roomId: RoomId.general))

    let bob = await peers.signIn(userId: AccountId.bob)
    let bobScreen = bob.render(RoomPage(roomId: RoomId.general))
    await bobScreen.settle()

    ana.textField(named: "Message").type("Lunch?")
    ana.button(named: "Send").click()
    await bobScreen.settle()

    expect bobScreen.listItem(containing: "Lunch?").isVisible
    expect ana.list(named: "In this room").text.contains("Bob")
}
```

This extends "a test is one browser talking to one server" from [21-testing.md](21-testing.md) to "one or more browsers talking to one server"; a test that never asks for a peer is unchanged. Each peer has its own browser container, so its client-only doubles (`TestConnectivity`, `TestMediaDevices`, …) are its own: `let bobsNetwork: TestConnectivity = bob.double()` (the double's type comes from the expected type, as for `client()`) and `bobsNetwork.goOffline()` take only Bob offline. A call test in [46-media-and-rtc.md](46-media-and-rtc.md#tests) joins its second participant through a `TestPeer`'s protocol client.

### `TestChannel<C, S, E>`: a scripted channel

For a client service tested against a fake protocol, with no server code at all:

| Member | Meaning |
|---|---|
| `channel: Channel<C, S, E>` | the value a fake's rpc returns |
| `deliver(_ message: S)` | the server sends a message |
| `sent: List<C>` | the messages the client sent, in order |
| `refuseNext(with: E)` | the next `send` throws `E` |
| `drop(resumable: Bool = true)` | as `TestChannels.drop`; a restart calls the fake's rpc again with the same arguments |
| `close(_ reason: ChannelClose)` | the server closes the channel |

- **`screen.settle()` treats an effect waiting for a channel's next message as idle,** after every delivered message has been handled, as for streams ([29-server-streaming.md](29-server-streaming.md#tests)). A `send` waiting for an acknowledgement is not idle.
- **Presence and broadcasts are in-process in tests,** and lease expiry follows the `TestClock`, so "a crashed replica's members leave after 30 seconds" is a test that advances the clock after `network.drop(resumable: false)`.
- **`WebSocketClient` has a hermetic default, `TestWebSocketClient`:** `serve(url:with:)` scripts a fake endpoint (`TestWebSocketServer`: `received`, `send(_:)`, `close(code:)`); any connection to an unscripted URL fails the test, naming the URL, like `TestHttpClient`.

## Visibility in development and production

- **`k dev` logs each channel open and close** on one line: method, user id, duration, messages each way, and the close reason. Message bodies are logged only at debug level, in development, with `Secret` fields redacted.
- **`k dev` serves an inspector at `/_k/dev/channels`**: open sockets and channels by session, each channel's state, sequence numbers, buffer depth and last messages, and the presence of each key. It is never in a `k build`.
- **`k routes` lists channel rpcs** with `CHANNEL` in its `PAGE` column, after their URL.
- **`k.observe` metrics:** open sockets and channels per method (gauges), messages in and out, refusals by case, acknowledgement latency (histogram), closes by reason, resumes versus restarts, broadcast delivery lag across replicas, and skipped unknown cases. Each open and each handled message is a trace span.

## Server-only and client-only

| Declaration | Side |
|---|---|
| a handler returning `Channel(sending:receiving:)`, `Broadcast`, `Presence`, `ChannelConfig` | server only |
| the `Channel` value's `state`, `messages`, `send`, `close`; `ChannelState` | client (and tests) |
| `ChannelClose`, `PresenceChange`, the app's `C`, `S`, `E` | both (they cross the wire) |
| `WebSocketClient` | both; `WebSocketRequest.header` server only |

A channel rpc is server-only code, so a static-only app ([38-deployment.md](38-deployment.md#static-only-apps)) that adds one gets a `dist/app`. **Channels between services** (a `remote` client of a channel rpc) are not supported yet, like streams between services ([40-workspaces-and-services.md](40-workspaces-and-services.md)).

## Example app: `examples/huddle`

A team chat, small enough to read in an afternoon, that exercises every part of this design and needs no account anywhere:

- **Sign-up with an email and a password,** rooms, messages stored in the database, edit and delete of one's own messages (`notYourMessage`), and a `Role(.moderator)` who may delete anyone's: per-message authorization in both forms.
- **Presence** ("In this room"), **typing indicators**, an unread count per room fed by a per-account `Broadcast<AccountId, RoomActivity>` channel in the layout, and a **reconnect banner** driven by `connection`.
- **Optimistic sends:** a pending message with a client id, replaced by its echo, marked failed with a retry action when `send` throws `.offline`.
- **Development:** `k dev` with SQLite. **Deployment:** a `docker-compose.yml` with Postgres, **two replicas** behind nginx with plain round robin, so messages and presence cross replicas over `LISTEN`/`NOTIFY`, and `docker compose restart app1` shows channels restarting on the other replica with no lost messages.
- **Tests:** two-peer tests for posting, presence and moderation; `TestChannels.drop` for the banner, resume and restart; `TestChannel` for `RoomService` alone.

A later step makes `examples/kanban` collaborative (shared boards with live card moves and cursors) on the same pieces, once kanban has a server.

## Decided while implementing

**Round 3, first slice: declarations, checks, `rpc.lock` and the frame codec.** Nothing opens a socket yet: the Rust WebSocket front, the server runtime, the client state machine, `TestChannels`/`TestPeer`/`TestChannel`, presence, `NOTIFY` fan-out, `WebSocketClient` and `examples/huddle` are later slices.

**Stdlib (`k.rpc`)**

- **`Channel<C, S, E>` is a `@Native` class** with `init(sending: Stream<S>, closed: ((ChannelClose) -> Void)? = nil, receiving: (C) async throws E -> Void)`, `let state: ChannelState`, `let messages: Stream<S>`, `func send(_ message: C) async throws E` and `func close()`. Nothing implements them yet (a call reaches `missingNative`). It declares **no `where` constraints** on `C`, `S` and `E`, so a wrong type argument is reported once, by the rpc rules below with a message about the channel, rather than as a generic constraint error as well.
- **`ChannelState`** (`Equatable`) and **`ChannelClose`** (`Equatable, Codable`) are the enums above. **`ChannelConfig`** is a stdlib `config` with `K_CHANNEL_MAX_MESSAGE` (bytes, default 65,536), `K_CHANNEL_RESUME_WINDOW` (30 s), `K_CHANNEL_HEARTBEAT` (25 s) and `K_CHANNEL_RATE` (messages a second, 20); the burst of 50 isn't configurable. It isn't added to `dist/app config` until the server runtime reads it.
- **The sides are member-level, so they are checks, not annotations.** `Channel` itself crosses the boundary (a client service holds one; a handler builds one), so it can't be `@ServerOnly` or `@ClientOnly`; K5511 and K5512 below draw the line instead. `ChannelState` isn't `@ClientOnly` either: it is reached only through `channel.state`, which K5511 already keeps off the server, and marking it would make a shared declaration that mentions it client-only. `ChannelConfig` is server-only as every `config` is.
- `Stream.merge(_:_:)` was left to the server runtime slice, which added it (below).

**Checks** (sema.md §12.8; [diagnostics](../compiler/diagnostics.md)):

- **The messages reuse the rpc codes,** since they are the same rules applied to the result: a non-`Codable` `C` or `S` is **K5503** at the type argument ("a channel's client message crosses the wire…"), an `E` without exactly one `rpcFailure(RpcFailure)` case is **K5502** (with the same fix that adds the case), and a non-`Codable` refusal payload is K5503. A `Secret` in `C` or `S` is K5503 because `Secret` isn't `Codable`; inside a struct that claims `Codable` it is K2009 at the struct, as for any wire type. No separate "server-only type in a message" rule was needed.
- **K5509 `channel-result-misplaced`:** a `Channel` (or `Channel?`) result of anything but a protocol's rpc or a service rpc that serves one, including a client service's helper and a server service's private helper; and a `Channel?` result of a channel rpc, which throws instead of answering `nil`.
- **K5510 `nested-channel`:** a type argument that names `Channel` anywhere a type is written (`List<Channel<…>>`, `Map<K, Channel<…>?>`, `Stream<Channel<…>>`, a local's annotation). A plain `Channel<…>?` is allowed, for the client service's field.
- **K5511 `channel-on-server`:** in a server-only declaration (as `di::side` computes it: a protocol's implementation, a `.request` or `.server` service, an endpoint page, anything injecting one), reading `state` or `messages`, calling `send` or `close`, or calling a channel rpc (which also covers a `remote` client of a channel rpc, since remote clients are server-only).
- **K5512 `channel-built-outside-handler`:** `Channel(sending:receiving:)` outside the body of a service's channel rpc handler, its lambdas included. A private helper that builds the channel for the handler is outside too (and K5509 for its result): the handler builds and returns it, as the design says.
- **K5513 `channel-opened-during-ssr`** (a warning): a `@Load` initializer that calls a channel rpc, or an app function that calls one through any chain of app functions. Pages and components can't inject protocols, so the direct case only arises with other errors; the useful case is a client service's method that opens a channel, called from a `@Load`. It is a whole-program pass over resolved calls after the bodies, skipped in a client-rendered app and a client-rendered component's subtree.
- **`/_k/channels`** as a page's or endpoint page's URL is the existing **K5001** (`url-conflict`), with its own message.
- **Narrowing:** the receiving lambda is part of the handler's body, so `session.user` is narrowed there under `requires Authenticated` with no new rule; the corpus pins it. Handler shape needed nothing new either: K5505 compares `Channel<C, S, E>` with the protocol's, and the initializer's types make `sending:` a `Stream<S>` and the lambda a `(C) async throws E`.

**`rpc.lock`** (format in [sema.md §12.16](../compiler/sema.md#1216-lockfiles))

- A channel rpc is written with `kind = "channel"` and `send`, `receive`, `refusal` **instead of** `returns`; other rpcs have no `kind`, so every existing lock reads and writes unchanged (a stream is told from a unary rpc by `returns = "Stream<…>"`, as before).
- **The one direction-dependent rule** is case removal: an enum reached in the program only through channels' `receive` types may lose a case. Reaching it from anything else, in any served protocol (a parameter, a result, an error, a `send`, a `refusal`), makes it strict. Added required fields and changed types break both directions, as the table says. Turning a channel into an rpc or a stream, or back, and changing `send`, `receive` or `refusal`, are each a K8001 note.
- **The comparison now follows enum case payloads,** using the program's payload types (the lock records only case names). Before, a struct carried only inside an error enum's case was never compared; with event enums as channel messages that gap mattered, so it is closed for every rpc.

**The wire** (`k_server::channels`, `runtime/channels.js`, part `Channels`)

- **Frames as built:** the JSON form is exactly the design's (`{"kind": {…}}`, keys in the order shown, numbers as JSON numbers up to 2^53 − 1, `ack.error` left out when accepted, `session.user` `null` for a guest); unknown keys inside a frame are ignored, an unknown kind is an error. The binary form is the Protobuf message `k.channel.v1.Frame`, a `oneof` of the ten kinds with payloads as `bytes` (the method names the payload's type) and the close reason as an enum numbered from 1; it is spelled out in `k_server::channels`. Encoding is canonical proto3, except `ack.error` and `session.user`, which are `optional` so an empty refusal is still a refusal.
- **Golden vectors** are `crates/k_runtime/js/tests/channel-frames.json`: each frame's decoded value and its exact JSON text and Protobuf bytes, plus invalid frames, read by `crates/k_server/tests/channels.rs` and `channels.test.js` (the JS test runner now loads a `name.json` beside the tests as the module `k:/tests/name.json.js`). The binary vectors were computed by a separate throwaway encoder written for them, not by either codec, so they don't merely restate one.
- **Channel numbers** are 1 to 2^32 − 1 (0 is "absent" in Protobuf, so it is invalid); sequence numbers start at 1, and `rcvd`/`resume` counts may be 0. The JS binary decoder rejects numbers above 2^53 − 1, which the Rust one accepts; no Kobra peer sends one.
- The `close` reasons are `ChannelClose`'s four. "Going away" on `SIGTERM` and the buffer-overflow drop are socket closes, not frames, so they need no reason here; the front's slice confirmed it (below).

**Round 3, second slice: the server** ([server.md §20](../compiler/server.md#20-channels)). The WebSocket front (`k_server::sockets`) and the channel runtime (`k:runtime/app`) serve channel rpcs in one process, tested with a raw WebSocket client (`crates/k_server/tests/channel_server.rs`, project `tests/projects/channels`). The browser's client state machine, `TestChannels`/`TestPeer`/`TestChannel`, presence, `NOTIFY` fan-out between replicas, `WebSocketClient`, tooling (logs, inspector, `k routes`, metrics) and `examples/huddle` are later slices.

- **The front is hyper's upgrade with `tokio-tungstenite` framing** (`=0.29.0`, the tests' client is `tungstenite` at the same version). The handshake is checked before anything runs: `Origin` first (`403`), then the upgrade headers and the `k.channel.v1` subprotocol (`400`); `503` once shutdown began. The upgrade itself needs no session: a guest may open a channel whose rpc has no `Authenticated` guard, and every open runs the guards under the handshake's cookie.
- **Events reach the isolate through its request queue, not through new `op_k_channel_next`/`_ack` ops.** A channel event is a request with `channel: {kind, id, seq, reason}` (`open`, `msg`, `close`), answered through `op_k_respond` with `{"ok": true}` or `{"error": …}`; only sending and server-side closing are new ops (`op_k_channel_send`, `op_k_channel_close`). That reuses the worker loop and its panic handling. **In-order handling is the front's:** a per-channel pump task hands the isolate one message and waits for its answer before the next, so the isolate needs no queue of its own, and the acknowledgement is sent when the receiving function returned, after its transactions committed.
- **Refusals are Connect error bodies.** `refused.error` and `ack.error` carry exactly what the same refusal answers over HTTP (`{"code", "message", "details"}`: `failed_precondition` with the `k.TypedError` detail for `E` or the open error, `unauthenticated`, `permission_denied`, `invalid_argument` with `k.ValidationError`, `resource_exhausted`), with `retryAfter` in seconds where HTTP has a `Retry-After` header, so the browser decodes both with the rpc client's `rpcError`. The vectors' `{"notAMember": {}}` stays an opaque example for the codec.
- **A message's `value` is its type's codec JSON** (`{"case": "posted", "text": "hi"}` for an enum, a struct's fields), the same encoding `Broadcast` uses between isolates; the design's `{"snapshot": {…}}` above was illustrative.
- **v0 speaks JSON frames only.** A binary frame closes the socket (`1003 k.binary`), as `application/proto` is `415` for rpcs; Protobuf frames arrive with the Protobuf wire format.
- **Socket close codes** (a socket close is never a channel close; the client resumes or restarts each channel): `1001` with the reason `k.retry=<ms>` at shutdown, the delay drawn from 0–5 s; `1002 k.protocol` for a malformed, server-only or out-of-sequence frame or a channel number in use; `1003 k.binary`; `1009 k.too-large`; `1013 k.overflow` when a channel's outgoing buffer overflowed: that channel's token is forgotten (it restarts after the client's backoff) while the socket's other channels resume; `4000 k.silent` after the silence limit. **The overflow drops the socket, not just the channel,** because no `ChannelClose` reason fits a channel the client must restart, and a socket close already means "resume what you can".
- **An unknown resume token** (past the window, after a deploy or an overflow, on another replica, or presented by another session's socket) is answered `refused {ch, error: {code: "not_found"}}`; a `refused` after a `resume` tells the client to restart. `resume.sent` isn't used: the client re-sends what wasn't acknowledged and the server repeats the remembered acknowledgement (the last 64 per channel) of a message it already handled. Tokens are `<process tag>.<worker>.<id>.<128 random bits>`, bound to the session cookie.
- **A resumed channel stays on its isolate,** whichever worker its new socket is pinned to: pinning decides where a socket's *new* channels open (the worker with the fewest open channels, then the fewest sockets).
- **Limits as built:** the frame text may exceed `maxMessage` by 1 KiB of envelope before the socket is closed (a Kobra client checks the message itself); the rate limit is a token bucket in Rust (rate a second, burst 50) and a message over it is acknowledged with `resource_exhausted` without reaching the isolate; **a socket over its session's 16 is accepted, and every open on it is refused** as rate-limited, rather than failing the handshake, so the refusal reaches the caller typed as the design says. The silence limit (60 s) isn't a `ChannelConfig` field; `ServerOptions::channels` sets every limit for tests.
- **Server-side `closed` reasons:** `byClient` after the client's `close` and when a resume window passes unresumed, `byServer` when the sending stream ends and at shutdown, `sessionEnded` when the session does. A channel ended at shutdown sends nothing; its socket's "going away" is the client's signal.
- **Sessions:** each message slides the session's idle timeout (the session is read again); the session's end reaches every channel opened under it through the same `k.session|<id>` broadcast streams use, and the socket gets `session {user: null}` once.
- **`Stream.merge(_:_:)` is a static member of `Stream<T>`** (`$k.streamMerge`, both sides), reading both sources concurrently; closing the merged stream closes both.
- **`ChannelConfig` is in the server's configuration** (and so `dist/app config`) when the app serves a channel rpc, and the front reads it (`ChannelLimits::from_config`).
- **Shutdown** stops new handshakes, closes every socket with `1001`, ends every channel in its isolate, then waits within `K_SHUTDOWN_GRACE` until no receiving function is running, before the HTTP connections' graceful close. hyper-util's `GracefulShutdown` can't watch a connection with upgrades, so each connection task watches a shutdown signal instead.


**Round 3, third slice: the browser's client** ([runtime.md §8.2](../compiler/runtime.md#82-channels), `runtime/channels.js`, the `channels` feature chunk). A client service's channel rpc call opens a real channel on `/_k/channels`; `TestChannels`, `TestPeer`, `TestChannel`, presence, `NOTIFY`, `WebSocketClient`, tooling and `examples/huddle` are later slices.

- **The stub** calls `openChannel(path, request, $c, {encode, decode, refusal})` (codegen.md §15) and returns the open `Channel`, or throws the open error through `rpcError` as any rpc does: a refused open's error body is the same Connect body as over HTTP, so nothing new decodes it. `Channel`'s members are the runtime object's own (`state` and `messages` read, `send($c, m)`, `close()`). The first open has the rpc's 30-second timeout and isn't retried.
- **One socket per page,** opened for the first channel and closed after the last one closes, so an idle page holds no socket. Channel numbers count up per page and are never reused, on any socket.
- **`connecting` covers every reopen:** a channel goes `connecting` when the socket is back and it has sent `resume` or `open`, and `open` at the server's `opened`; `reconnecting(attempt:at:)` is the time without a socket. A closed channel's `messages` ends, and leaving a `for await` over it early closes the channel, as for a stream.
- **Backoff as designed** (full jitter up to 0.5 s … 30 s, whole milliseconds) from the client's random source, `Math.random`, which `k test` seeds per test, so timings repeat. A `1001` waits exactly the server's `k.retry=<ms>`; `1002`, `1006`, `4000` and every other close are ordinary drops. **After `1013 k.overflow` the next successful open doesn't reset the attempt count,** so a channel that can't keep up backs off further each time instead of restarting every half second.
- **The timers are the browser's,** not the injected `Clock`: the stub has no container to inject one from. The runtime's `channelEnv(…)` replaces the socket factory, time, timers, random source and online check; `TestChannels` (next slice) will point it at the `TestClock`.
- **Connectivity, both ways, through window events:** a failed attempt dispatches `k-offline` and an opened socket `k-online`, which `Connectivity` follows; the client retries at once on `online`, `k-online` and the document becoming visible, and a due attempt doesn't run while `navigator.onLine` is `false`. Neither chunk imports the other.
- **Receipts:** `rcvd` with the last received number a second after a message (at most once a second) and with each reply to a server `beat`; the reply to a `beat` is a `beat`. No client-initiated heartbeat is needed, since the server's arrives every 25 s.
- **Sending:** a message gets its sequence number when it is transmitted, not when `send` is called, so a restart numbers the waiting ones from 1 on the new channel; at most 32 transmitted and unacknowledged, the rest wait. The 64 KiB check is on the UTF-8 length of the message's codec JSON, and fails `rpcFailure(.invalid(…))` with one failure at path `""`. An `ack` with `unavailable` (the isolate had already ended the channel) is `rpcFailure(.offline)`.
- **Restart:** a `refused` with `not_found` after a `resume` opens the channel again under a new number; any other `refused` then closes it `.refused`. A `refused` of the first open is the call's error.
- **A change of session restarts every channel at once on a new socket,** without resuming: the hub watches the mirror's user and replaces the socket when it changes (a sign-in or sign-out by rpc, or the socket's own `session {user: null}`, which goes through the same `takeMirror` as an `x-k-session` header, so the router's gate sees it first).
- **The back-forward cache:** `pagehide` drops the socket and schedules nothing; `pageshow` reconnects at once, resuming.
- **An unknown `S` case** is skipped with a console warning in every build, not only in development: a warning costs nothing in production and makes a stale tab diagnosable. The `k_channel_unknown_total` metric came later (round 4, "The unknown-case metric").
- **Opening a channel on the server** (an in-process call while rendering) panics in `callLocal` with "channels open only in the browser: open it from an `@Effect`".
- **Budget:** the chunk (`CHANNELS_SIZE_BUDGET`, measured compacted with both codecs) is 4.5 KB: 2 KB for the codecs as before, 2.5 KB for the client, about the rpc client and the streams chunk together for a state machine that does more than both. A release build links the unused Protobuf codec out: the shipped chunk of the test project is 3.2 KB gzip. The full client runtime's budget is unchanged (the client reuses `takeMirror`, now exported, rather than adding a hook).
- **Tests:** `crates/k_runtime/js/tests/channel-client.test.js` drives the client against a scripted server (fake socket, virtual timers, the seeded random source, the window's events), case by case through this section; `chrome::channels_resume_in_chrome_after_the_socket_drops` (`crates/k_cli/tests/chrome.rs`) runs `tests/projects/channels`, which gained a `Room` page and a client `RoomService`, under `k dev` in real Chrome: open, send, the echo, the socket closed from DevTools while new sockets are refused, an announcement published meanwhile, and the resume that delivers it exactly once.
- **Fixed on the way:** `session.user?.id.name` read `.name` from `nil` (the nil test wrapped only the next link of the chain); a nil now skips the whole rest of a chain (codegen.md §6.1), and the test project's `if let` workaround is gone.

**Round 3, last slice: the test helpers and `examples/huddle`.** Channels are user-facing now: the book's [Channels](../book/src/apps/channels.md) chapter and [Huddle](../book/src/examples/huddle.md) page. Presence, `NOTIFY` between replicas, `WebSocketClient` and the tooling (`k routes`, the inspector, metrics, the open/close log lines) are still to come; the book names them as Planned (`channel-presence`, `broadcast-replicas`, `websocket-client`, `channel-tooling`).

- **An in-process front** (`k:/testing/channels.js`, `createFront`) stands where `k_server::sockets` stands in production. The channels' client (`runtime/channels.js`) talks to it through a fake WebSocket that `channelEnv` installs per test, in the real JSON frames; it hands `open`, `msg` and `close` to the app's handler exactly as the Rust front does (`request.channel`), and the kit points the app's `op_k_channel_send`/`op_k_channel_close` at it. It numbers and buffers server messages, trims them on `rcvd`, keeps a dropped socket's channels for the 30-second window on the `TestClock`, repeats remembered acknowledgements, applies the token-bucket rate limit, and sends a heartbeat every 25 s. Heartbeats are delivered synchronously, because one `clock.advance` can pass many of them while microtasks wait, and the client would otherwise declare the socket silent; other frames arrive a microtask later, in order. A socket the client closes is dropped after the frames it sent before closing, as a real socket sends them. Server channel ids are never reused across tests, so a channel the app still holds from an earlier test can't be mistaken for a new one.
- **`channelEnv` grew the browser:** `browser()`, `mirror(b)` and `take(b, text)`, and `socket(b)`/`online(b)` take it. A page is one browser (`null`), so production keeps one socket per page; in a test each `TestPeer` gets its own socket, cookie and `Session` mirror. The client keeps a hub per browser.
- **`screen.settle()` keeps waiting while channel traffic moves:** a broadcast's next element reaches JavaScript only after a turn of the event loop, and what the front sends then needs another, so with a front `settle` runs at least two idle rounds and more until a round moves no frame (the front counts them). Effects waiting for a channel's next message are idle, as for streams.
- **`TestChannels`** (`@Inject`, no `use` rule; it replaces no interface) has `drop(resumable: Bool = true)` and `open: List<OpenChannel>`, where **`OpenChannel`** is `{method, arguments}` with the arguments as the request message's JSON (one type for every channel rpc, which a typed field couldn't be). `drop()` is a `1006` close; `drop(resumable: false)` also ends the server's channels at once (their `closed` runs with `byClient`, as when a window passes), which is what "the server forgets it" is in-process. **`TestConnectivity.goOffline()` drops the test's socket** (resumable) and `goOnline()` dispatches the window's `online` event, which the client already follows; the kit's window now has real `addEventListener`/`dispatchEvent`, new per test.
- **`TestPeers`/`TestPeer`, narrower than designed:** `peers.signIn(userId:roles:) async -> TestPeer` and `peers.guest()`, and on the peer `client<P>() -> P` (the browser's stub of `P`, bound to the peer: its calls set the active browser while they start, and every rpc and channel starts its request synchronously) and `signOut()`. **A peer has no `render` and no `double()`:** the browser runtime is one module graph with one document, one router and one `Session` mirror, so a second rendered browser would need a second runtime instance; protocol clients cover the two-person tests (one person on the page, the other through the protocol), which is what `examples/huddle` needs. The peer's rpcs carry its cookie, and an `x-k-session` answer updates its mirror, not the test's.
- **Signing out in the peer's own browser closes its guarded channels with `.refused`, not `.sessionEnded`:** the mirror changes first, the client moves the channels to a new socket under the guest session, and their restart is refused, exactly as in a real tab. `.sessionEnded` is what a session ended elsewhere (another tab, `endAll`, expiry) gives, and the fixture pins both.
- **`TestChannel<C, S, E>`** is a `@Native` class constructed as `TestChannel()` with the type from the expected type; codegen hands the kit `E`'s constructor so a closed channel's `send` throws `E.rpcFailure(.offline)`. `deliver`, `sent`, `refuseNext(with:)`, `drop(resumable:)` and `close(_:)` as designed, with one simplification: **a drop reopens after the first backoff (500 ms of the `TestClock`), and a restart doesn't call a fake's rpc again**, since the scripted channel has no rpc behind it: messages delivered while dropped are replayed on a resume and lost on a restart, and the test delivers the new snapshot itself. Sends wait while dropped, at most 10 s, as on a real channel.
- **The front's limits are the defaults,** not `ChannelConfig`'s values: a test of a limit belongs to `channel_server.rs`, which shortens them.
- **`examples/huddle`, round-3 scope:** sign-in with a name (no password: `examples/notes` shows accounts), three fixed rooms, messages stored in SQLite (Postgres in production) and ordered by a per-room number assigned in the posting transaction, deleting one's own messages (`notYourMessage`), optimistic sends with a client id and a **Retry** for `.offline`, and a reconnect banner from `channel.state`. No presence, typing indicators, unread counts, moderators, `docker-compose.yml` or replicas yet: they need presence and `NOTIFY`. `k dev` is the one command, as for the other examples. Its `k test` is in `commands.rs`, and `examples.rs` builds its `dist/app` and talks to a room over a raw WebSocket (open, snapshot, post with echo and acknowledgement, a refusal).
- **Tests:** `crates/k_cli/tests/fixtures/realtime` (17 tests: resume without loss, restart with a new snapshot, offline, hours of virtual time with heartbeats, `open` before and after the page is left, a session ended by sign-out and by `endAll`, peers posting, refused and guest peers, and a service against a `TestChannel`), and huddle's 13.


**Round 4, first slice: presence and replicas** ([server.md §20, "Presence and replicas"](../compiler/server.md#presence-and-replicas)). `WebSocketClient`, `Backoff` and the tooling are still to come (Planned `websocket-client`, `channel-tooling`); the book's Planned `channel-presence` and `broadcast-replicas` are gone.

- **Stdlib (`k.rpc`):** `Presence<Key, Member>` (`@ServerOnly @Native`, `Key` and `Member` `Hashable` and `Codable`) with `join(key:member:) -> PresenceLease<Member>`, `members(key:) async -> List<Member>` and `watch(key:) -> Stream<PresenceChange<Member>>`; `PresenceLease<Member>` (`@ServerOnly @Native`) with `update(member:)` and `leave()`; `PresenceChange<Member>` (`Codable, Equatable`, cases `joined(Member)`, `left(Member)`, `updated(Member)`), which crosses the wire as `{"case": "joined", "_0": member}` like any enum with an unlabelled payload. No new checks or diagnostic codes: `Presence` is server-only like `Broadcast`.
- **"The request that took it" is found at run time, not passed.** `join` is synchronous and usually called from a `.server` service's method, which has no request and no `$c`, so the runtime enters each request (`createApp`'s dispatch, a channel's open and every receiving call) into a deno_core `AsyncVariable`, which V8 carries across `await`s; `join` reads it. The request's leases end after its response, at the end of a stream, and for a channel when it ends (closed, refused, session ended, forgotten, or the resume window passed). A lease taken where there is no request (a job) lasts until `leave()`.
- **Counting:** a member is its codec JSON, compared as text. `members` lists each member once, in the order of its first live lease. `update` moves one lease to a new member: if the old member has no lease left and the new one had none, that is `.updated(new)` (keeping the old place in `members`); if the old one is gone and the new one was already present, `.left(old)`; if the old one stays and the new one is new, `.joined(new)`; otherwise nothing.
- **Without Postgres** leases live in the process (every worker isolate shares them) and need no expiry: a crashed process takes its members with it.
- **In `k test`** presence is in-process, and a lease's end follows the in-process front: `drop(resumable: false)` ends it at once, and a socket that doesn't come back ends it when the 30-second resume window passes on the `TestClock` (`TestConnectivity.goOffline()` keeps it away). There is no "crashed replica" in `k test`, so the 30-second expiry of a replica's leases is tested in Rust against Postgres (`crates/k_server/tests/replicas.rs`), not with the `TestClock` as the design's Testing section suggested.
- **Replicas** (`k_server::replicas`, started only when `DATABASE_URL` is Postgres): a publish is delivered in its own process at once and sent as `NOTIFY k_broadcast, {"p": tag, "k": key, "v": element}` on the statement connection, in call order; over 7,000 bytes the payload is stored in `k_broadcast_spill` (`id`, `payload`, `created_at`) and the notification is `{"p", "s": id}`. A listening connection of its own delivers other processes' elements in arrival order (a spill is fetched before the next is delivered), skips its own tag, and reconnects after a second; notifications sent while it was down are lost, which a channel's restart snapshot already covers. A starting server waits up to 5 seconds for its first `LISTEN`, so a replica that serves already hears the others.
- **Presence with Postgres** is `k_presence` (`lease`, `key`, `member`, `process`, `joined_at`, `expires_at`): join, update and leave each run in a transaction under `pg_advisory_xact_lock(hashtext(key))`, so two replicas joining the same member announce `joined` once. Each process renews its rows every 10 s to 30 s ahead; each process also deletes expired rows (of any process), and spills older than a minute, every 10 s, and announces the members left without a live lease. Times are the processes' clocks.
- **Ending a session reaches every replica,** which the design required (["Sessions and security"](#sessions-and-security)) and round 3 did only in-process: the replica that ends it notifies `{"p", "ended": [SHA-256 hashes of the cookie tokens]}`, never a token, and the others close the streams and channels opened under them.
- **`examples/huddle`, round 4:** presence ("In this room": `RoomPresence`, merged into the room's stream with `Stream.merge`, the snapshot gained `present`), typing indicators (`ChatCommand.typing`, `ChatEvent.typing(name:)`, sent at most every 3 s, shown 5 s on the injected `Clock`, cleared by the person's message or departure), and a `Dockerfile`, `docker-compose.yml` (Postgres, `app1`, `app2`, nginx) and `deploy/nginx.conf` with the design's lines and plain round robin. **Not yet:** unread counts per room, moderators and password sign-up; they need nothing new from the runtime and are the example's next step.
- **Huddle's replica test runs two `dist/app` processes against one Postgres** (Docker, or `K_TEST_POSTGRES_URL`; skipped without them unless `K_REQUIRE_DOCKER=1`) over raw WebSockets, without nginx or the compose images: building the image compiles the toolchain, far too slow for a test. Presence spanning both replicas, a post through A reaching a socket on B, typing and a departure are checked; the compose file and nginx configuration are checked for their shape.
- **Found on the way, fixed in the next slice:** `rpc.lock` recorded an enum's case names but not its cases' payload fields, so huddle's `snapshot` gaining a required `present` wasn't flagged as the breaking `S` change the table above says it is. Huddle had never been deployed, so nothing broke; the lock now records them (below).
- **Tests:** `crates/k_cli/tests/fixtures/realtime/src/hall` (5 `k test` tests: counting across tabs, update and early leave, a lease of an rpc call, a resume that never left and a forgotten socket that did, the resume window passing), `crates/k_server/tests/replicas.rs` (5, real Postgres: order across replicas, a spill, presence counted once across replicas, expiry after a crash, a session end), huddle's 5 new `k test` tests, and `examples.rs`'s two replicas and deployment shape.

**Round 4, second slice: payload fields in `rpc.lock`, a deterministic sign-out, `WebSocketClient` and `Backoff`.** The tooling (`k routes`, the inspector, metrics, the open/close log lines) is still to come (Planned `channel-tooling`); the book's Planned `websocket-client` is gone.

*`rpc.lock` records case payload fields* ([sema.md §12.16](../compiler/sema.md#1216-lockfiles)):

- **Each enum case is an inline table,** `{ name = "snapshot", fields = [{ name = "messages", type = "List<chat.ChatMessage>" }, …] }`, and `{ name = "typing" }` for a case without a payload; an unlabelled payload is named `_0`, `_1`… by position, as the wire names it. Payload fields have no numbers: a payload is JSON on the wire today, and numbers come with Protobuf enums.
- **No format key, and none added.** A lock has never had one, and the new form is a superset the reader tells apart per case: a bare string (`cases = ["snapshot", "posted"]`) is a case recorded before payloads were, which still reads and whose fields, being unknown, aren't compared (its payload *types* are still followed through the program, as before). So every existing lock loads, and `k build` rewrites one in the new form the next time it writes it; the examples' locks were regenerated so.
- **A case's fields follow a struct field's rules, in both directions:** removed, retyped, or new and required is K8001 (`field `present` of case `snapshot` of `huddle.chat.ChatEvent` is new and required`). The direction still matters only for removing a whole case, which a type reached only through `receive` may do; a removed case's fields aren't compared. Removing a field of a `C` case could be argued compatible (an old tab sends a field the server ignores), but it is kept breaking, as for a struct, so the lock has one rule for fields.
- Corpus: `sema/rpc/lock/channel-case-payloads` (each change, in `C`, `S` and a refusal, with the compatible ones silent) and `case-payloads-legacy` (an old lock still reads, and still compares the structs it reaches).

*The flaky notes sign-out* (`chrome::notes_signs_up_and_creates_edits_and_deletes_a_note`): signing out ended the session, which ended the page's change stream with an empty end-of-stream envelope; on a loaded machine the page's `for await` saw that end before the sign-out's response reached the router, so `NotesService.watch()` opened the stream again under the dead session, and the `401` was a console error the test fails on. **A stream ended by its session now carries the guest mirror in the Connect end's metadata** (`{"metadata": {"x-k-session": ["{\"user\":null}"]}}`, [server.md §6](../compiler/server.md#6-rpc-dispatch)), which the browser takes as it takes the header, through the router's gate, before the stream ends: the guarded page is left before its code can reopen anything. This is what [29](29-server-streaming.md) and "Sessions and security" above already promise for channels' `session {user: null}`, now for streams too. With that fixed, a second race of the test itself showed: the guest mirror makes the router leave the guarded list for a full load, which the server redirects to the sign-in page, and the test followed the header's "Sign in" link (shown as soon as the session is gone) and typed while that load was under way; it now waits for the load. The test runs with a 4× CPU throttle (DevTools' `Emulation.setCPUThrottlingRate`), which reproduced the `401` on every run before the fix; 10 runs, each beside a full run of the other Chrome tests, passed.

*`WebSocketClient` and `Backoff`* (`k.http`, `k.time`; [runtime.md §12.13](../compiler/runtime.md#1213-raw-websockets), [server.md §18](../compiler/server.md#18-outgoing-http-and-email)):

- **Declarations:** `WebSocketClient` (`@Platform(side: .both)`, `connect(request:)`), its one implementation `StandardWebSocketClient` (no `use` rule), `WebSocket` (`@Native` class: `state`, `subprotocol`, `messages`, `send(_:)`, `close(code: Int = 1000, reason: String = "")`), `WebSocketMessage` (`text(String)`, `binary(Bytes)`, `json<T>() throws DecodingError`), `WebSocketState` and `WebSocketError` as designed. `WebSocket` gained `subprotocol` (what the endpoint chose), which a client of a versioned feed needs.
- **`WebSocketRequest` is a `@Builder` struct like `HttpRequest`:** `url`, `subprotocol: String? = nil`, `headers`, `secretHeaders`, `bearerToken`, `timeout = 30.seconds`, `maxMessage = 1_000_000`. The design's `.header(_:_:)` became `HttpRequest`'s three setters (`.headers([…])`, `.secretHeaders([…])`, `.bearerToken(_:)`), because a builder's setters are its fields and a two-argument setter would need a mechanism of its own; one subprotocol rather than a list, since the design's example asks for one.
- **K4031 `handshake-header-on-client`** (new): one of those three setters chained in a declaration that may run in the browser (shared or client-only, as `di::side` computes it), at the setter's name. A `Secret` can't reach client code anyway; the check is for plain `headers`, which a browser would silently drop.
- **`json()` throws `DecodingError`,** as `IncomingRequest.json()` does: `WebSocketError` has no decoding case, and a bad message is the app's to skip, not a socket failure.
- **The browser** (`runtime/websocket.js`, a feature chunk shipped only to apps whose client injects `WebSocketClient`, 1.5 KB budget): the browser's `WebSocket` with `binaryType = "arraybuffer"`. It can't tell a refused handshake from an unreachable host, so both are `offline`; and it can close only with 1000 or 3000–4999, so a socket closed for an oversized message closes with the browser's default and reports `closed(1009, "too large")`. `send` waits while `bufferedAmount` is over 1 MiB.
- **The server** is Rust (`k_server::websocket`): tokio-tungstenite's client over hyper-rustls's connector (TCP, or TLS with the Mozilla roots, as outgoing HTTP), within the request's timeout (at most 10 minutes); an answer that isn't an upgrade is `refused(status:)` with its `HttpStatus` case; a message over `maxMessage` (at most 64 MiB) is `tooLarge`, and the socket is closed with 1009. Ops `op_k_ws_connect`, `_next`, `_send`, `_close`; pings are answered in Rust. An offline handshake logs a warning with the URL without its query, never the headers.
- **Owned like an `HttpClient` call:** a cancelled `connect` never settles and drops the handshake; an open socket closes (1000) when the run that opened it is cancelled.
- **`TestWebSocketClient`** (`@TestDefault`) and **`TestWebSocketServer`** (`TestWebSocketServer()`; `received`, `connections`, `send(_:)`, `close(code:reason:)`) as designed, with `serve(url:with:)` taking the server and `fail(url:error:)` added, like `TestHttpClient`'s. URLs match without their query. **One double per test, shared by the test's browser and server** (the kit and the server's `perTest` use the same memo), so a test scripts what a server service bridging a feed connects to; a connection closed by either side is confirmed at once.
- **`Backoff`** is a `@Native` class (`Backoff()`, `next() -> Duration`, `reset()`), the channels' policy exactly: the cap table and the full-jitter draw are one function in `std.js` (`backoffDelay`) that the channels' client now calls too, so the two can't drift. It draws from `Math.random`, which `k test` seeds per test; the delays are slept on the caller's `Clock`, so `TestClock` drives them.
- **Not done:** the `k_channel_unknown_total`-style metrics for sockets, and `TestWebSocketServer` scripting per connection (it broadcasts to every open one), which no example needs yet.
- **Tests:** `crates/k_server/tests/websocket_client.rs` (the Rust client against a local echo server, `fakes::FakeWebSocket`: echo of text and bytes, subprotocol and headers, close codes both ways, too large, refused with its status, offline without secrets, timeout, invalid URLs; and the server's JavaScript client over the real ops against the echo server), `crates/k_runtime/js/tests/websocket.test.js` (the browser's client over a fake `WebSocket`, the server's over stand-in ops, the doubles, `Backoff`, `json()`), `crates/k_cli/tests/fixtures/feeds` (7 `k test` tests: a page following a scripted feed, a reconnect after the backoff on the `TestClock`, unmounting closes, a server service sharing the double, a refusal, `Backoff`, `json()`), the corpus `sema/di/websockets` (K4031, and the API's types), and the book's [Raw WebSockets](../book/src/apps/http.md#raw-websockets) section.

**Round 4, third slice: the tooling** (`k routes`, the log lines, metrics and spans, the inspector). The book's Planned `channel-tooling` is gone; its [Channels](../book/src/apps/channels.md#seeing-what-channels-do) chapter has a section on it.

- **`k routes`:** a channel rpc of a served protocol is a row after every page, endpoint and redirect (sorted by URL among themselves), with `WS` as the method, the wire URL, `CHANNEL (ChatProtocol.room)` in the `PAGE` column, the rpc's parameters as written and the protocol's then the rpc's guards. `WS` because the row is opened on the socket, never requested at that URL; unary and streaming rpcs stay out of the table, which is about URLs a browser requests.
- **The development tools are one switch, `ServerOptions::dev_tools`,** which `k dev` and `k run` set beside `dev_errors` (both serve a development build) and `dist/app` never does. It turns on the log lines, the recent messages kept per channel, and the inspector route; nothing of it is compiled into the app, so "never in a `k build`" holds by construction and `examples.rs` checks huddle's `dist/app` answers `/_k/dev/channels` as an unknown page.
- **Log lines** are the front's (Rust), at info level, one per open, refused open and close: `channel /m opened for {"name":"ana"}`, `channel /m closed (byClient) for {"name":"ana"} after 42.0 s: 3 in, 17 out`, `channel /m refused: closed (chans.chat.RoomError.closed)`. The user is the session's user id as its JSON, which the isolate now returns with an accepted open (`{"ok": true, "user": "…"}`, left out for a guest); a refused open has none. **Message bodies are not logged at debug level:** the server's output has no level filter, so a debug line per message would flood `k dev` in a chat with typing indicators; the inspector's last messages show bodies instead. (A `Secret` can't be in a message anyway, since `Secret` isn't `Codable`.) The hub keeps the last 100 lines for the inspector and for tests (`Hub::log_lines`).
- **Metrics live in a registry of the process** (`k_server::metrics`), held by the channels' hub and recorded by the front: `k_channel_sockets` (gauge), `k_channel_open{method}` (gauge of opened channels, those waiting for a resume included), `k_channel_messages_total{method, direction="in"|"out"}`, `k_channel_refusals_total{method, case}` (the typed refusal's case, or the Connect code for the rest: `resource_exhausted`, `invalid_argument`, `unavailable`, `internal`), `k_channel_ack_seconds{method}` (histogram from a message's arrival in the front to its acknowledgement, rate-limited ones included), `k_channel_closes_total{method, reason}` (`byClient`, `byServer`, `sessionEnded`, `refused` for a refused open, and two the client never sees as reasons: `expired` when a resume window passes and `overflow`), `k_channel_reconnects_total{outcome="resumed"|"restarted"}` (a `restarted` is a `resume` with an unknown token, whose method the server doesn't know, so it has no method label) and `k_broadcast_lag_seconds` (each element from another replica: its notification now carries `"t"`, the publisher's Unix milliseconds, which older replicas ignore; the lag is by the two processes' clocks). Histograms share fixed buckets from 1 ms to 10 s.
- **Spans are records in the same registry,** the last 256: `k.channel.open` (from the `open` frame to the isolate's answer; `method`, `channel`, `outcome` = `opened` or the refusal's case) and `k.channel.message` (from arrival to acknowledgement; plus `seq`). There is no trace context to join yet.
- **Exposition:** the registry renders the Prometheus text format, which the inspector shows; nothing exports it. Export to a collector is the book's Planned `observe-metrics`, which already covered metrics and tracing in `k.observe`; a `/metrics` endpoint on a deployed app was not added, since choosing between a scrape endpoint and OpenTelemetry push is that feature's decision. Tests read the registry through `Server::channels().metrics` (`counter`, `gauge_value`, `histogram`, `spans`, `render`). The task's "existing observe test helpers" don't exist: `k.observe` had only `Log`.
- **`k_channel_unknown_total` was not recorded in this slice** (the browser had no way to report a skipped case); it is now, below.

**The unknown-case metric** (built with the full client runtime's size round, docs/compiler/performance.md §11.3).

- **The receipt carries the count:** `rcvd` gained `skipped`, the messages of an `S` case the tab didn't know that it skipped since its last `rcvd` (JSON `"skipped"`, left out when 0; Protobuf `uint64 skipped = 3`, so 0 is absent as usual). A skipped message still counts as received, so the next receipt always follows it within the second. No new frame kind: older servers ignore the key (unknown keys inside a frame are ignored) and the field, and a frame without it means 0, so old tabs and old servers interoperate.
- **The server adds it to `k_channel_unknown_total{method}`** (the channel's method; `sockets.rs`' `received`), next to the other channel metrics. The console warning stays, for the tab's own developer.
- **Size:** the channels chunk went from 4,358 B to 4,409 B gzip (budget 4,608 B); the full client runtime didn't change.
- **Tests:** the golden vectors (`channel-frames.json`: a `rcvd` with `skipped`, and a negative count refused) on both codecs, `channel-client.test.js` ("the receipt after a skipped case reports how many were skipped") and `channel_server.rs`' `skipped_unknown_cases_are_counted_from_receipts`.
- **The inspector** (`k_server::devtools`, `GET /_k/dev/channels`): one server-rendered page that refreshes itself every two seconds (a `<meta http-equiv="refresh">`, no script). Sockets are grouped by session, shown as the first 8 hex digits of the cookie's SHA-256 (never the cookie) or `guest`; each channel shows its number and id, method, user, state (`opening`, `open`, `waiting for a resume` with the window's time left), worker, age, the last sequence number each way, unconfirmed messages and bytes, messages each way, and its last 8 messages (cut at 300 characters). Then the channels waiting for a resume, presence per key with each member's lease count (in-process presence; with Postgres it says presence is in `k_presence`), the recent log lines and the metrics. It is the same for `k run`, and isn't behind the request-forgery check since it is a `GET` with no effect.
- **Tests:** `crates/k_server/tests/channel_server.rs` (`channels_record_metrics_and_spans_for_opens_messages_refusals_and_closes`, `dev_tools_log_opens_and_closes_and_serve_the_inspector`), `metrics.rs`'s and `devtools.rs`'s unit tests, `process.rs`'s presence snapshot, `replicas.rs`'s lag across two replicas, `commands.rs`'s `routes-huddle.txt`, `cli.rs`'s `k dev` serving the inspector, and `examples.rs`'s huddle `dist/app` without it. No Chrome test: the page has no script, so an HTTP test covers it.

**`examples/huddle`: unread counts, moderators and password rooms** (2026-11-11). Written as an app author would with today's Kobra: no language or runtime feature was added, and three compiler bugs found on the way were fixed, each test-first.

- **The unread count is a second channel, the lobby's** (`LobbyProtocol.lobby()`), not the layout's per-account channel the example section above sketched, and not a server stream: it lives on the page that shows it, and a restart brings fresh counts in its snapshot, as a room's does. It merges an all-rooms key and a per-account key of one `Broadcast<String, LobbyEvent>` (`RoomActivity`). Its `C` is `create(name:password:)`, so creating a room is refused over the channel with `LobbyError`.
- **Counts are kept in the browser between snapshots** from `posted`/`deleted(room:number:author:)` and the server's own count in `read(room:upTo:unread:)`; comparing numbers with `lastRead` makes their order across replicas not matter. "Seen" is applied in a visible tab (`PageVisibility`), sent as `ChatCommand.read(upTo:)`.
- **The lobby passes on only the rooms the person is a member of** (a private `followed` list per channel, extended by their own `read`s), so a password room's activity reaches nobody else. A sync `Stream.filter` with a side effect on the channel's own state did this, as there was no async filter. **Superseded:** `filter` now takes an async function ([29](29-server-streaming.md#stream-operators)), and the lobby asks the database for each post, so a removal counts at once and the per-channel list is gone.
- **Moderators are per room** (`Membership.moderator`, the creator first), not the `Role(.moderator)` sketched above: a role is global, and a room's creator moderating it needs no administrator. Every action is checked against the database for each command, so a change on another replica counts at once.
- **Removing a member** couldn't close their channel from the server (K5511: a server reads no `Channel` members), so their channel's sending stream passed nothing after their own `removed(name:)`, their `PresenceLease` was left, further commands were refused `removedFromRoom`, and their tab closed the channel. **Superseded:** the stream ends with `prefix(through:)` after the removal ([Ending one channel from the server](#ending-one-channel-from-the-server)), which closes the channel `byServer` and leaves the lease; the per-channel flag and filter are gone, and a command that arrives first is refused `removedFromRoom` by the database's membership.
- **A password is given once to a unary rpc**, `unlock(room:password:)` with `requires RateLimit(5.perMinute)`, not as a channel parameter: a channel's `RateLimit` limits opens, which would count every reconnect.
- **Fixed on the way** (each with a corpus case first): a view's `if let x = …, <condition on x>` read `x` before declaring it in the client (`codegen/ui/views/if-let-condition-reads-its-binding`); `break` in a `match` arm lowered to a `switch` left only the switch, so the loop ran on (`run/control-flow/break-in-a-match-arm-leaves-the-loop`); and a stdlib type reached only from an app codec's payload had no codec in the client, so **every `PresenceChange` was skipped in a real browser** and huddle's "In this room" never updated live outside `k test`, whose bundle shares the server's (`codegen/split/presence-changes-decode-in-the-client`, and the Chrome test). Also in the example: message numbers were the count plus one, so a number was reused after a deletion; they are a per-room counter now.

**Language friction, round B** (2026-11-11), from the huddle report above:

- **The flaky lobby count** (`chrome::huddle_counts_unread_live_and_a_moderator_deletes_for_everyone_in_chrome`, "#team 1 unread" for someone who had just read it) was the app's: `LobbyService` is an app-scoped client service, so coming back to the lobby showed the counts kept on the earlier visit until the new channel's snapshot arrived, and those had stopped being kept (a `read` told while the lobby wasn't open reaches no lobby). Under load the snapshot came after the test's check. `follow()` now starts with an empty list, so the lobby shows only counts its own channel keeps; Lobby.test.k's "coming back to the lobby never shows the counts it had when it was left" holds the snapshot back with `TestConnectivity.goOffline()`, which failed before the fix.
- **Ending one channel from the server** is the section above, with `Stream.prefix(through:)`.
- **Fixed on the way:** `while let x = await …` lowered only plain conditions, so the loop read `x` without declaring it (a `ReferenceError` in huddle's new moderation test); a `while` condition now lowers through the same `let`-condition code as `if` (`run/control-flow/while-let-awaits-its-value`).

## Why

- **An rpc that returns a `Channel` reuses everything.** Guards, typed errors, narrowing, wire names, `rpc.lock`, validation, layering and test doubles already exist for rpcs; a separate `socket` declaration would have needed a second copy of each, and a second set of security rules to get wrong.
- **Three type parameters are the three questions** a realtime feature has: what can I say, what will I hear, and why can what I said be refused. A refusal that is a typed error on `send`, rather than an event the app has to correlate, keeps [09-errors.md](09-errors.md)'s rule that every failure is visible in a signature.
- **Per-connection state as the request container's state** gives handlers state without a new scope or a state machine, and sequential handling makes it race-free.
- **Resume-or-restart is honest.** Resuming within a window is lossless and invisible; past it, the runtime re-runs the handler instead of pretending, and snapshots make that correct. Failing a transmitted-but-unacknowledged message on restart follows the rpc rule that the runtime never guesses whether a handler ran.
- **Fan-out through `Broadcast`** means one mechanism, already shared across isolates and replicas, already deferred to commit, already filtered at publish time.
- **No third-party account and no new infrastructure:** in-process for one replica, Postgres when the app already has it, a provider interface beyond that.

## Alternatives rejected

- **A `channel` keyword or a `socket` declaration with `on message` handlers.** More grammar, and every rpc rule (guards, locks, narrowing) would need restating. The return type already says what kind of call it is.
- **Exposing raw WebSockets to app code for its own client and server.** That is the pain the user described: reconnect loops, heartbeat timers, JSON parsing, auth on upgrade and cross-site hijacking, rewritten in every app. Raw sockets stay for third-party endpoints only.
- **Socket.IO-style string event names and untyped payloads.** They lose exhaustiveness and lock files.
- **One channel carrying many subscriptions (`subscribe(topic)` messages).** It turns authorization into a per-subscription protocol the app writes; one channel per subject keeps it one open-time decision.
- **A persistent, replay-forever outbox.** Commands replayed after a long outage act on a world that has moved on. A short bound plus a typed `.offline` lets the UI ask.
- **A server-side replay log in the database for resume across replicas.** It would cost a write per message for every app, to save a snapshot on the rare reconnect that lands elsewhere.
- **A `Channel<C, S>` whose refusals are events.** It would move correlation into every app, and lose the typed error on `send`.
- **Carrying server streams on the socket too.** Possible later (open question), but streams work everywhere today and share a connection across tabs; changing their transport isn't needed to ship channels.
- **Requiring sticky sessions at the load balancer.** It would make the default deployment depend on proxy configuration. Stickiness only makes resumes more frequent.

## Open questions

- Per-channel overrides of the limits (`Channel(sending:limits:receiving:)`) for a channel that legitimately sends bigger or faster messages, such as drawing.
- Whether server streams should ride the socket when one is already open, saving a connection per stream.
- Channels between services, with the server-to-server streaming client.
- Whether a stale client (an old bundle) should be told to reload when the server's `rpc.lock` has moved past it, rather than only skipping unknown cases.
- Binary payloads (`Bytes` in `C` or `S`) larger than the message limit: chunking, or always an rpc.

Resolved while reconciling with [45](45-web-platform.md) and [46](46-media-and-rtc.md): a refused restart carries no typed error on the channel (call the rpc again); receipt is confirmed with `rcvd` frames so the outgoing buffer holds only unconfirmed messages; going offline in tests is `TestConnectivity`'s, not `TestChannels`'; and WebRTC signalling is an ordinary channel rpc ([46](46-media-and-rtc.md#signalling-over-a-channel)), not a new kind of declaration.
