# Huddle

A small team chat written in K: sign in with a name, pick a room, and talk. Messages arrive live in every tab over a **channel**, survive a dropped connection, and are stored in SQLite (Postgres in production). Each room shows who is in it and who is typing, across every replica of the app.

It is a teaching example for channels ([`docs/design/44-channels.md`](../../docs/design/44-channels.md)), the typed, two-way connection between a browser tab and its server, and for Web Push ([`docs/design/50-push.md`](../../docs/design/50-push.md)): a room's **Notify me of new messages** tells you about new messages while no tab is open. It is deliberately small: unread counts and moderators come with later steps, so they aren't here yet.

## Running it

| Command, at `examples/huddle` | What it does |
|---|---|
| `k dev` | Builds and serves the app on `http://localhost:8080`, with a SQLite file at `.k/huddle.db`, created and migrated at startup. Open it in two browsers (or a normal and a private window), sign in with two names and post in the same room. |
| `k test` | Runs the tests: two people in one room, a dropped and a forgotten connection, offline sends, and the client service against a scripted channel. |
| `k check` | Checks the code, runs the tests, and checks `rpc.lock` against the protocols. |
| `k build` | Builds `dist/`, with a `dist/app` that serves the pages, the rpcs and the channels' WebSocket. |
| `docker compose up --build --wait` | Runs it as in production: Postgres, two replicas (`app1`, `app2`) and nginx on `http://localhost:8080`. See [Two replicas](#two-replicas). |

To try push notifications, give the server a VAPID key: run `k keys new p256-ecdsa` and put the private JWK it prints in `.env` as `K_PUSH_PRIVATE_KEY`, with `K_PUSH_CONTACT=mailto:you@example.com` (see [Push notifications](#push-notifications)). Without a key the switch isn't shown.

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
| **Presence**: who is in a room, from a `Presence<String, Member>` whose leases end with the channel; two tabs of one person are one entry | `src/chat/RoomPresence.k`, `src/chat/ChatApi.k`, `src/chat/RoomService.k` |
| `Stream.merge` of the room's events and its presence changes into one sending stream | `src/chat/ChatApi.k` |
| **Typing indicators** as an ordinary command and event, shown for 5 seconds on the injected `Clock` | `src/chat/RoomService.k`, `src/chat/Room.k` |
| Two replicas behind nginx sharing messages and presence over Postgres `LISTEN`/`NOTIFY` | `docker-compose.yml`, `deploy/nginx.conf`, `Dockerfile` |
| Sign-in with `SessionManager` and the `Authenticated` guard on the whole protocol | `src/accounts/AuthApi.k`, `src/chat/ChatProtocol.k` |
| **Web Push**: a "Notify me" switch that subscribes with `PushSubscriptions` and the server's key, subscriptions stored per room, and a background job that pushes each new message with `WebPush`, deleting expired subscriptions | `src/chat/RoomAlerts.k`, `src/chat/AlertSwitch.k`, `src/chat/AlertsApi.k`, `src/chat/RoomAlertJob.k` |
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

### Who is here

1. `ChatApi.room(name:)` watches `RoomPresence` and merges its changes into the channel's stream (`Stream.merge`), **then** joins the room with `Member(name:)` and reads who is present for the snapshot, so nobody arriving in between is missed.
2. The lease belongs to the channel's request: when the channel closes (the page is left, the session ends, or a dropped connection doesn't come back within 30 seconds), the person leaves, with no cleanup code. A resumed channel never left, so a blip doesn't flicker.
3. Ana in two tabs holds two leases of the same `Member`: the room sees one `joined` when the first opens and one `left` when the last closes.
4. `RoomService` keeps `present` from the snapshot and the `joined`, `left` and `updated` events; the page lists it as "In this room".

**Typing** isn't presence. The page sends `ChatCommand.typing` while the draft changes, at most once every 3 seconds; the server passes `ChatEvent.typing(name:)` to the room; `RoomService` shows a name until 5 seconds after its last event, or until that person's message arrives. Nothing is stored, so nothing needs cleaning up.

### Reconnecting

The runtime owns the connection; the page only shows `RoomService.banner`, which is derived from `channel.state`.

- **A short drop** (a laptop lid, a train tunnel) resumes within 30 seconds: the server kept the channel, replays what the tab missed and skips what it already handled. Nothing is lost or handled twice.
- **A long drop or a restarted server** restarts the channel: the guard and the handler run again, and the new snapshot brings the room up to date.
- **A message sent while disconnected** waits at most 10 seconds. If it couldn't be delivered, it stays in the list marked **Not sent.** with a **Retry** button, so the person decides whether to send it again: after a restart the runtime can't know whether the server handled it.

### Push notifications

A room's **Notify me of new messages** (`AlertSwitch`, rendered in the browser only) calls `RoomAlerts.turnOn(room:)` from the click: `PushSubscriptions.requestAccess()` shows the notification prompt the first time, the server's key comes from `AlertsProtocol.serverKey()` (the public half of `K_PUSH_PRIVATE_KEY`), `subscribe(serverKey:)` subscribes the browser, and `follow(room:subscription:)` stores a `RoomAlert` for the signed-in person, the room and the browser's endpoint. **Stop notifying me** deletes it.

Posting a message enqueues a `RoomAlertJob` with the message, in the transaction that stores it. The job pushes `"ana in #general"` and the text to the room's followers, except the author and anyone in the room right now (`RoomPresence`), with the tag `room-<name>`, so one notification per room stays on screen, and `opens: .room(name:)`, so a click opens the room. A subscription the push service says is gone (`WebPushError.expired`) is deleted; an unavailable push service retries the job with backoff.

The browser's push service (Google's for Chrome, Mozilla's for Firefox, Apple's for Safari) carries the message, encrypted so it can't read it; the app is identified by its own key, with no account. In production set `K_PUSH_PRIVATE_KEY_FILE` to a file holding the key.

## Two replicas

`docker-compose.yml` runs Postgres, two replicas of the app and nginx with plain round robin; `deploy/nginx.conf` passes the WebSocket upgrade (`proxy_http_version 1.1`, `Upgrade`, `Connection "upgrade"`, `proxy_read_timeout 75s`). Nothing needs configuring for the replicas to talk:

- every `Broadcast` publish is delivered on its own replica and sent to the others with Postgres `NOTIFY`, so a message posted through `app1` reaches a tab connected to `app2`;
- presence leases live in the `k_presence` table, renewed by their replica every 10 seconds; a crashed replica's members leave 30 seconds later;
- sessions are in the database, so a cookie issued by one replica works on the other.

`docker compose restart app1` shows the rest: the tabs on `app1` show **Reconnecting…**, reconnect through nginx to `app2` and restart their channels there with a new snapshot. `crates/k_cli/tests/examples.rs` runs two replicas of `dist/app` against one Postgres and checks that a message and presence cross between them, and `K_COMPOSE=1 cargo test -p k_cli --test it -- compose::huddle` runs this compose file itself: two people signed in through nginx, on different replicas, see each other's message ([testing.md §8.6](../../docs/compiler/testing.md#86-compose-smoke-tests)).

The first run of that test found the two replicas racing at startup: both created the runtime's own tables (sessions, jobs, presence) at once, and one stopped with `unique violation on pg_type` and was restarted by Compose. `dist/app` now creates those tables under the same Postgres advisory lock as the migrations ([38-deployment.md](../../docs/design/38-deployment.md#commands)), so both replicas start once.

## Structure

```
App.k                    the app: default layout, sign-in page, metadata
k.toml                   project manifest
Dockerfile               the image: k check, k build, dist/app on distroless
docker-compose.yml       Postgres, two replicas, nginx
deploy/nginx.conf        round robin with the WebSocket upgrade
.env.example             optional overrides for `k dev`; copy to the git-ignored .env
schema.lock              database schema, generated by `k migrate`
migrations/
  0001_create_messages.k
  0002_create_room_alerts.k
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
    Member.k               struct: someone in a room
    RoomPresence.k         @Scope(.server) Presence of each room's people
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
    AlertsProtocol.k       protocol: the server's push key, follow and unfollow a room
    AlertsApi.k            server implementation
    RoomAlert.k            entity: a browser following a room
    RoomAlertRepository.k  repository
    NewMessageAlert.k      struct: the job's payload
    RoomAlertJob.k         job: pushes a new message to the room's followers
    RoomAlerts.k           client service: the browser's subscription and the switch's state
    AlertSwitch.k          component: "Notify me of new messages"
    *.test.k               tests
  shell/
    AppLayout.k            layout: the app's name and who is signed in
    HuddleTheme.k          theme
```

## Testing

Channels run in `k test` in-process, through the real frames, guards, handlers and acknowledgements, with no socket and no second process:

- **`TestPeers`** opens a second browser with its own session against the same server, database and clock: `let bob = await peers.signIn(userId: AccountId(name: "bob"))`, then `let chat: ChatProtocol = bob.client()`. `Room.test.k` uses it for "a message posted by one person appears for another", "only a message's author can delete it", presence ("the people in the room are listed, and leave when their channel closes") and typing.
- **`TestChannels`** controls the test's own connection: `network.drop()` drops it (it resumes after its backoff on the `TestClock`), `network.drop(resumable: false)` makes the server forget it (it restarts with a new snapshot), and `network.open` lists the open channels.
- **`TestConnectivity.goOffline()`** drops the connection and stops reconnecting until `goOnline()`.
- **`TestPushSubscriptions`** and **`RecordingWebPush`** replace the browser's push subscription and the server's sender: `AlertSwitch.test.k` turns a room's notifications on and off and checks that a posted message is handed to the job (`RecordingJobQueue`), and `RoomAlertJob.test.k` runs the job: the author and the people in the room aren't told, an expired subscription is deleted, an outage is retried, a server without a key fails the job.
- **`TestChannel<C, S, E>`** is a scripted channel for testing `RoomService` without a server: `RoomService.test.k` delivers events, refuses the next message, drops and closes it.

## Design documents used

- [44-channels.md](../../docs/design/44-channels.md): channels, reconnecting, limits and testing
- [29-server-streaming.md](../../docs/design/29-server-streaming.md): `Broadcast` and snapshots
- [38-deployment.md](../../docs/design/38-deployment.md): `dist/app`, images and replicas
- [26-auth-and-sessions.md](../../docs/design/26-auth-and-sessions.md): `SessionManager` and guards
- [21-testing.md](../../docs/design/21-testing.md): tests on the real stack in-process
- [50-push.md](../../docs/design/50-push.md): `PushSubscriptions`, `WebPush` and the service worker
- [30-background-jobs.md](../../docs/design/30-background-jobs.md): `RoomAlertJob`
