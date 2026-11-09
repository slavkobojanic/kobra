# Huddle

A small team chat written in K: sign in with a name, pick a room, and talk. Messages arrive live in every tab over a **channel**, survive a dropped connection, and are stored in SQLite.

It is a teaching example for channels ([`docs/design/44-channels.md`](../../docs/design/44-channels.md)), the typed, two-way connection between a browser tab and its server. It is deliberately small: who is in a room, typing indicators, moderators and running several replicas come with later steps of the design, so they aren't here yet.

## Running it

| Command, at `examples/huddle` | What it does |
|---|---|
| `k dev` | Builds and serves the app on `http://localhost:8080`, with a SQLite file at `.k/huddle.db`, created and migrated at startup. Open it in two browsers (or a normal and a private window), sign in with two names and post in the same room. |
| `k test` | Runs the tests: two people in one room, a dropped and a forgotten connection, offline sends, and the client service against a scripted channel. |
| `k check` | Checks the code, runs the tests, and checks `rpc.lock` against the protocols. |
| `k build` | Builds `dist/`, with a `dist/app` that serves the pages, the rpcs and the channels' WebSocket. |

To see a reconnect, stop `k dev` while a room is open and start it again: the page shows **Reconnecting…**, then catches up from the new snapshot.

## What it demonstrates

| Feature | Where |
|---|---|
| A **channel rpc**: `rpc room(name:) throws RoomError -> Channel<ChatCommand, ChatEvent, CommandError>` | `src/chat/ChatProtocol.k` |
| The handler: guards and an open-time check (`noSuchRoom`), a snapshot then live events from a `Broadcast`, and a receiving function that runs once per message, in order | `src/chat/ChatApi.k`, `src/chat/RoomEvents.k` |
| Per-message refusals as a typed error on `send` (`invalidText`, `notYourMessage`), with the channel staying open | `src/chat/ChatApi.k`, `src/chat/CommandError.k` |
| A broadcast published inside a transaction, delivered only once the message is stored | `src/chat/ChatApi.k` |
| A client service that follows a channel with `for await`, applies events idempotently, and shows `channel.state` as a banner | `src/chat/RoomService.k`, `src/chat/Room.k` |
| Optimistic sends: a pending message with a client id, replaced by its echo, marked failed with **Retry** when it can't be delivered | `src/chat/RoomService.k`, `src/chat/PendingMessage.k`, `src/chat/Room.k` |
| A page that opens the channel from an `@Effect`, so leaving the page closes it | `src/chat/Room.k` |
| Sign-in with `SessionManager` and the `Authenticated` guard on the whole protocol | `src/accounts/AuthApi.k`, `src/chat/ChatProtocol.k` |
| Two-person tests with `TestPeers`, connection tests with `TestChannels` and `TestConnectivity`, and a scripted `TestChannel` | `src/chat/Room.test.k`, `src/chat/RoomService.test.k` |

## How it works

### Joining a room

1. The `Room` page starts `RoomService.follow(room:)` from an `@Effect(on: [name])`. It runs in the browser only; leaving the page cancels it, which closes the channel.
2. `follow` calls `chat.room(name:)`. That one call opens the channel on the tab's WebSocket (`/_k/channels`, shared by every channel of the tab). The protocol's `Authenticated` guard and the handler run on the server; a refusal, such as `RoomError.noSuchRoom` for `/room/nowhere`, is thrown by the call itself.
3. `ChatApi.room(name:)` subscribes to the room's `Broadcast` **before** reading the stored messages, so a message posted in between arrives after the snapshot instead of being lost. It returns `Channel(sending:)` with the snapshot first, and a receiving function.
4. `RoomService` reads `opened.messages` with `for await`, and applies each event. Applying is idempotent: a message that arrives in a snapshot and again as an echo is shown once, in the order of its `number`.

### Posting

1. `RoomService.post(text:)` adds a `PendingMessage` with a random client id, so the message shows at once as **Sending…**.
2. `channel.send(.post(text:clientId:))` waits for the server's acknowledgement. On the server the receiving function runs once per message, in order, inside one channel's request: it checks the text, stores the message in a transaction and publishes `.posted(message:clientId:)`, which is delivered when the transaction commits.
3. Every channel of the room receives the echo, the sender's included. The sender replaces its pending copy, matched by the client id.
4. A refused message (`invalidText`) throws from `send`; the page puts the text back in the field and says why. The channel stays open.

### Reconnecting

The runtime owns the connection; the page only shows `RoomService.banner`, which is derived from `channel.state`.

- **A short drop** (a laptop lid, a train tunnel) resumes within 30 seconds: the server kept the channel, replays what the tab missed and skips what it already handled. Nothing is lost or handled twice.
- **A long drop or a restarted server** restarts the channel: the guard and the handler run again, and the new snapshot brings the room up to date.
- **A message sent while disconnected** waits at most 10 seconds. If it couldn't be delivered, it stays in the list marked **Not sent.** with a **Retry** button, so the person decides whether to send it again: after a restart the runtime can't know whether the server handled it.

## Structure

```
App.k                    the app: default layout, sign-in page, metadata
k.toml                   project manifest
.env.example             optional overrides for `k dev`; copy to the git-ignored .env
schema.lock              database schema, generated by `k migrate`
migrations/
  0001_create_messages.k
src/
  accounts/              module accounts: signing in and out
    AccountId.k            struct: the session user's id, the name they chose
    AuthProtocol.k         protocol: signIn(name:), signOut()
    AuthApi.k              server implementation, using SessionManager
    AuthService.k          client service; exposes the session user
    SignInError.k          enum
    SignIn.k               page
  chat/                  module chat: rooms and messages
    ChatProtocol.k         protocol: the room(name:) channel rpc
    ChatApi.k              server implementation: the handler and its receiving function
    ChatCommand.k          enum: what the browser sends (C)
    ChatEvent.k            enum: what the server sends (S)
    CommandError.k         enum: why a message was refused (E)
    RoomError.k            enum: why a room can't be joined
    RoomEvents.k           @Scope(.server) Broadcast of each room's events
    Message.k              entity (server-only)
    MessageRepository.k    repository
    ChatMessage.k          struct: a message as the browser sees it
    PendingMessage.k       struct: a message on its way
    Rooms.k                the fixed list of rooms
    RoomService.k          client service: follows a room, optimistic sends
    Lobby.k                page at "/": the rooms
    Room.k                 page at "/room/{name}": one room, live
    *.test.k               tests
  shell/
    AppLayout.k            layout: the app's name and who is signed in
    HuddleTheme.k          theme
```

## Testing

Channels run in `k test` in-process, through the real frames, guards, handlers and acknowledgements, with no socket and no second process:

- **`TestPeers`** opens a second browser with its own session against the same server, database and clock: `let bob = await peers.signIn(userId: AccountId(name: "bob"))`, then `let chat: ChatProtocol = bob.client()`. `Room.test.k` uses it for "a message posted by one person appears for another" and "only a message's author can delete it".
- **`TestChannels`** controls the test's own connection: `network.drop()` drops it (it resumes after its backoff on the `TestClock`), `network.drop(resumable: false)` makes the server forget it (it restarts with a new snapshot), and `network.open` lists the open channels.
- **`TestConnectivity.goOffline()`** drops the connection and stops reconnecting until `goOnline()`.
- **`TestChannel<C, S, E>`** is a scripted channel for testing `RoomService` without a server: `RoomService.test.k` delivers events, refuses the next message, drops and closes it.

## Design documents used

- [44-channels.md](../../docs/design/44-channels.md): channels, reconnecting, limits and testing
- [29-server-streaming.md](../../docs/design/29-server-streaming.md): `Broadcast` and snapshots
- [26-auth-and-sessions.md](../../docs/design/26-auth-and-sessions.md): `SessionManager` and guards
- [21-testing.md](../../docs/design/21-testing.md): tests on the real stack in-process
