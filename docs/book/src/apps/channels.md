# Channels

A **channel** is a long-lived, two-way, typed conversation between one browser tab and the app's
server: chat, live cursors, game input, anything where the browser also talks often. It is declared
as an rpc whose result is **`Channel<C, S, E>`**: the browser sends messages of type `C`, the server
sends messages of type `S`, and the server may refuse one browser message with the error enum `E`.
The runtime owns the hard parts: one WebSocket per tab for every channel, heartbeats, reconnecting
with backoff, resuming without loss after a short drop, size and rate limits, and closing everything
when the session ends. The design is in `docs/design/44-channels.md`, and
[Huddle](../examples/huddle.md) is the example app.

## Rpc, stream or channel

| Need | Use |
|---|---|
| ask once, get an answer: load, save, search | an **rpc** ([Protocols and RPC](protocols.md)) |
| follow what the server changes while the browser only listens: a live list, a dashboard | a **server stream**, `Stream<T>` ([Server streaming](streaming.md)) |
| the browser also sends often, small and fast: chat, cursors, typing, game input | a **channel** |

- **Streams stay on HTTP.** They cross every proxy and need no socket. A channel adds a socket,
  acknowledgements and per-connection state; it isn't a better stream.
- **Request and response stays an rpc,** even next to an open channel. A channel message has no return
  value: what the server answers goes out on the channel to everyone who should see it, or comes back
  as the refusal `E`.

## Declaring a channel

A channel rpc is an rpc like any other, so the protocol's guards, wire names, `rpc.lock` and typed
errors all apply to it. Only its result type is new:

```k
// examples/huddle/src/chat/ChatProtocol.k
public protocol ChatProtocol requires Authenticated {
    /// Joins a room until the caller leaves it: a snapshot first, then
    /// every event.
    rpc room(name: String) throws RoomError -> Channel<ChatCommand, ChatEvent, CommandError>
}
```

- **`C` and `S` are `Codable`,** usually enums with one case per kind of message, so both sides `match`
  them exhaustively. Huddle's are `ChatCommand` (`post`, `delete`, `typing`) and `ChatEvent`
  (`snapshot`, `posted`, `deleted`, `presence`, `typing`).
- **`E` is an error enum with an `rpcFailure(RpcFailure)` case,** like every rpc error (K5502 without
  one); `RpcOnlyError` serves a channel that refuses nothing of its own.
- **The rpc's own `throws` is for opening:** `RoomError.noSuchRoom` is thrown by the call that opens the
  channel. `E` is what each message can be refused with.
- A `Channel` is the result of a protocol rpc or its handler only (K5509), never inside another type
  such as `List<Channel<…>>` (K5510).

## The server: the handler

The handler runs once, when the channel opens, after the guards. It returns a `Channel` built from the
messages it will send and a function that receives the browser's:

```k
// examples/huddle/src/chat/ChatApi.k
    rpc room(name: String) async throws RoomError -> Channel<ChatCommand, ChatEvent, CommandError> {
        guard Rooms.names.contains(name) else {
            throw RoomError.noSuchRoom
        }

        // Subscribe before reading, so a message posted or a person arriving
        // in between shows after the snapshot instead of being lost.
        let live = Stream.merge(
            events.subscribe(room: name),
            presence.watch(room: name).map { change => ChatEvent.presence(change) }
        )
        let me = session.user.id.name

        // In the room until this channel closes: leaving the page, a
        // dropped connection that doesn't come back, or signing out.
        presence.join(room: name, member: Member(name: me))

        let stored = await messages.all(room: name)
        let present = await presence.members(room: name)

        return Channel(sending: live.startingWith(.snapshot(messages: stored.map(chatMessage), present: present))) { command =>
            try await handle(command: command, room: name, author: me)
        }
    }
```

- **`sending:` is every message the server sends,** usually a `Broadcast` subscription
  ([Server streaming](streaming.md#sharing-events-between-requests)) started with a snapshot. Rooms and
  topics are `Broadcast` keys; channels add no fan-out of their own. `Stream.merge(_:_:)` combines two
  sources, here the room's events and its [presence](#presence-who-is-here). When the stream ends, the
  server has closed the channel.
- **The trailing function runs once per browser message, in order, one at a time per channel.** Its
  acknowledgement is sent when it returns, after any transaction in it has committed. Throwing `E`
  refuses that one message, and the channel stays open:

```k
// examples/huddle/src/chat/ChatApi.k
            case .delete(let id) {
                guard let message = await messages.find(id: id, room: room) else {
                    throw CommandError.messageGone
                }

                guard message.author == author else {
                    throw CommandError.notYourMessage
                }

                await database.transaction {
                    await messages.delete(message)
                    events.publish(room: room, event: .deleted(id: id))
                }
            }
```

- **The channel's request lasts as long as the channel.** The implementing service's instance, its
  request-scoped services and its `Session` live until the channel closes, so a private `var` of the
  service is per-channel state and needs no lock.
- **Throwing from the handler refuses the open,** as for any rpc. Only the handler builds a `Channel`
  (K5512), and server-only code never reads one (K5511).
- **The sender gets its own echo.** A `Broadcast` reaches every subscriber, the sender's channel
  included, so apply events idempotently and match an echo to a pending message by an id the browser
  chose (Huddle's `clientId`).

## The browser: following a channel

Calling the rpc from a client service opens the channel and returns it:

| Member | Meaning |
|---|---|
| `state: ChannelState` | reactive: `connecting`, `open`, `reconnecting(attempt:at:)`, `closed(ChannelClose)` |
| `messages: Stream<S>` | every server message, in order, across reconnects; ends when the channel closes for good |
| `send(_ message: C) async throws E` | sends and waits for the server's acknowledgement; throws the refusal, or `rpcFailure` |
| `close()` | closes the channel for good |

```k
// examples/huddle/src/chat/RoomService.k
    func follow(room: String) async throws RoomError {
        let opened = try await chat.room(name: room)

        await listen(on: opened)
    }

    /// Applies the events of an open channel until it closes.
    func listen(on opened: Channel<ChatCommand, ChatEvent, CommandError>) async {
        attach(opened)

        for await event in opened.messages {
            apply(event: event)
        }
    }
```

A page starts it from an `@Effect`, so the channel belongs to the page: leaving the page cancels the
effect, which closes the channel. Channels open only in the browser; a `@Load` that would open one is
the warning K5513.

```k
// examples/huddle/src/chat/Room.k
    @Effect(on: [name])
    func follow() async {
        problem = nil

        do {
            try await room.follow(room: name)
        } catch .noSuchRoom {
            problem = "There's no room called “{name}”."
        } catch .rpcFailure(.rejected) {
            Router.navigate(to: .signIn.returnTo(.room(name: name)))
        } catch .rpcFailure {
            problem = "Couldn't join #{name}. Check your connection and reload."
        }
    }
```

## Reconnecting

After the first open, the runtime owns the connection. The `for await` loop keeps going across drops,
and the page only shows the state:

```k
// examples/huddle/src/chat/RoomService.k
    @Derived let connection: ChannelState = channel?.state ?? .connecting

    /// What the banner says, or `nil` while the room is live.
    @Derived let banner: String? = match connection {
        case .connecting { "Connecting…" }
        case .open { nil }
        case .reconnecting { "Reconnecting…" }
        case .closed { "Disconnected. Reload the page to rejoin." }
    }
```

- **The first open behaves like an rpc call:** a guard's refusal, the handler's error, `.offline` and
  `.timeout` are thrown by the call, and it isn't retried.
- **A drop reconnects with backoff:** a random delay up to 0.5 s, then 1, 2, 4, 8, 16 and 30 seconds,
  at once when the browser comes back online or the page becomes visible, and not while it is offline.
- **Within 30 seconds, a channel resumes:** the server kept it, replays the messages the tab missed and
  skips those it already handled. Nothing is lost or handled twice, and the handler doesn't run again.
- **After that, or after a restarted server, it restarts:** the guards and the handler run again under
  the current session, so a handler that starts with a snapshot brings the page up to date with nothing
  else to write. A restart the guards refuse closes the channel with `.refused`.
- **A message sent while reconnecting waits,** at most 10 seconds and 32 messages, then `send` throws
  `rpcFailure(.offline)`. One that was transmitted but not acknowledged before a restart throws
  `.offline` too, because the old server may or may not have handled it; the app decides whether to send
  it again. Huddle keeps such a message in the list with a **Retry** button.
- **The session is the channel's:** signing out, expiry or `endAll` closes its channels (`.sessionEnded`
  when the session ended elsewhere), and a sign-in or sign-out in the tab moves its channels to a new
  socket under the new session.

## Presence: who is here

Who is in a room is the most-asked-for realtime feature and the easiest to get wrong: a crashed server
leaves ghosts, two tabs of one person flicker, and a tab closed without a goodbye stays "online".
**`Presence<Key, Member>`** in `k.rpc` does it once, held by a `@Scope(.server)` service like a
`Broadcast`:

```k
// examples/huddle/src/chat/RoomPresence.k
@Scope(.server)
public service RoomPresence {
    private let presence: Presence<String, Member> = Presence()

    /// Adds `member` to `room` until the channel (the request) that called
    /// this closes: nothing to clean up, even after a crash.
    func join(room: String, member: Member) -> PresenceLease<Member> {
        return presence.join(key: room, member: member)
    }
```

| Member | Meaning |
|---|---|
| `join(key:member:) -> PresenceLease<Member>` | adds `member` under `key` for as long as the calling request lives; for a channel, until it closes |
| `members(key:) async -> List<Member>` | everyone present, each `Member` once, in the order they arrived |
| `watch(key:) -> Stream<PresenceChange<Member>>` | `.joined(m)`, `.left(m)` and `.updated(m)` from now on |
| `lease.update(member:)` | replaces the lease's member (a status such as "away") and announces `.updated` |
| `lease.leave()` | leaves early; calling it again does nothing |

- **A lease belongs to the request that took it,** so the handler above writes no cleanup. The member
  leaves when the channel closes: the page is left, the session ends, or a dropped connection isn't
  back within the 30-second resume window. A resumed channel never left, so a blip doesn't flicker. A
  lease taken by an ordinary rpc ends with that call.
- **Members are counted, not listed:** two tabs of one person hold two leases with equal `Member`
  values, and the room sees one `joined` when the first arrives and one `left` when the last goes.
- **Watch before you join and read,** as the handler does, so no change between the two is missed. The
  joining channel hears its own `joined`, as it hears its own echo.
- **Typing isn't presence.** Huddle sends "typing" as an ordinary event and shows it for 5 seconds after
  the last one, on the injected `Clock`; nothing stores it, so nothing has to clean it up.

## Several replicas

Replicas of an app behind a load balancer share its database and nothing else. With Postgres, every
`Broadcast` and `Presence` reaches every replica with no configuration:

- **A publish is delivered on its own replica at once,** and to the others over Postgres
  `LISTEN`/`NOTIFY` on one channel per app. An element too large for a notification is stored for a
  minute in the stdlib's `k_broadcast_spill` table, and the notification carries its id.
- **Presence leases live in the `k_presence` table.** Each replica renews its own every 10 seconds;
  a lease not renewed for 30 seconds, because its replica crashed, expires on every replica, and its
  member leaves.
- **Ending a session closes its channels and streams on every replica,** wherever its socket is.
- **Sticky load balancing isn't needed.** A reconnect that lands on another replica restarts its
  channels there with a new snapshot instead of resuming them; nothing is lost that the snapshot
  doesn't bring back.

With SQLite or no database, an app is one process, and both stay in it. Huddle's `docker-compose.yml`
runs two replicas behind nginx on Postgres ([Huddle](../examples/huddle.md#two-replicas)).

## Limits

| Limit | Default | When it's exceeded |
|---|---|---|
| encoded message size, each direction | 64 KiB | `send` throws `rpcFailure(.invalid(…))` before transmitting |
| messages per second, per channel | 20, bursts of 50 | the message is refused with `rpcFailure(.rejected(.rateLimited(…)))` |
| messages in flight, per channel | 32 unacknowledged | `send` waits |
| waiting while reconnecting | 32 per channel, 10 s | `send` throws `rpcFailure(.offline)` |
| sockets per session; channels per socket | 16; 32 | the open is refused as rate-limited |
| resume window | 30 s | the channel restarts |

The settings are the stdlib's `ChannelConfig` (`K_CHANNEL_MAX_MESSAGE`, `K_CHANNEL_RESUME_WINDOW`,
`K_CHANNEL_HEARTBEAT`, `K_CHANNEL_RATE`), listed by `dist/app config` for an app that serves a channel.
A file doesn't belong in a channel: send it with an rpc or an endpoint page, and the event that it
changed on the channel. Behind a proxy, pass the `Upgrade` header through
([Deployment](deployment.md)).

Raw WebSockets to other services (**Planned** websocket-client) and channels in `k routes`, a `k dev`
inspector and metrics (**Planned** channel-tooling) come later.

## Testing

Channels run in `k test` in-process, through the real frames, guards, handlers, acknowledgements and
limits, with no socket and no second process. Three helpers of `k.test` drive them.

**`TestPeers` opens another browser** in the same test, with its own session, against the same server,
database and clock. `peer.client()` gives a protocol client whose rpcs and channels run as that
person:

```k
// examples/huddle/src/chat/Room.test.k
    test "a message posted by one person appears for another" {
        await sessions.signIn(userId: AccountId(name: "ana"))
        let screen = render(Room(name: "general"))
        await screen.settle()

        // Bob is a second browser with his own session, on the same server.
        let bob = await peers.signIn(userId: AccountId(name: "bob"))
        let chat: ChatProtocol = bob.client()
        let channel = try await chat.room(name: "general")
        try await channel.send(.post(text: "Lunch?", clientId: Uuid("6f1c2a2e-0000-4000-8000-000000000001")))
        await screen.settle()

        expect screen.item(containing: "Lunch?").text == "bob Lunch?"
```

**`TestChannels` controls the test's own connection:** `drop()` drops its socket, and the channels
resume after their backoff on the `TestClock`; `drop(resumable: false)` also makes the server forget
them, so they restart with a new snapshot; `open` lists the channels open now.
`TestConnectivity.goOffline()` drops the socket and stops reconnecting until `goOnline()`.

```k
// examples/huddle/src/chat/Room.test.k
        network.drop()
        await screen.settle()

        expect screen.status().text == "Reconnecting…"
// ...
        clock.advance(by: 1.second)
        await screen.settle()

        expect not screen.status().exists
```

**`TestChannel<C, S, E>` is a scripted channel** for a client service tested without a server: hand
its `channel` to the code under test, then `deliver` server messages, `refuseNext(with:)`, `drop()` and
`close(_:)` it, and read what was `sent`:

```k
// examples/huddle/src/chat/RoomService.test.k
    test "a post shows as pending until its echo replaces it" {
        let room: TestChannel<ChatCommand, ChatEvent, CommandError> = TestChannel()
        rooms.attach(room.channel)

        try await rooms.post(text: "Hello")
```

`screen.settle()` waits while channel traffic moves, and treats an effect waiting for a channel's next
message as idle, as for a stream.

**Presence runs in-process too,** and its leases end exactly as in production: with the channel, with
an rpc's call, at `drop(resumable: false)`, or when the resume window passes on the `TestClock` after
`TestConnectivity.goOffline()`. Huddle's room page lists Bob once though he has two tabs open:

```k
// examples/huddle/src/chat/Room.test.k
        // Bob, in two tabs: listed once.
        let bob = await peers.signIn(userId: AccountId(name: "bob"))
        let chat: ChatProtocol = bob.client()
        let tab = try await chat.room(name: "general")
        let other = try await chat.room(name: "general")
        await screen.settle()

        expect screen.list(named: "In this room").items().map { item => item.text } == ["ana", "bob"]
```
