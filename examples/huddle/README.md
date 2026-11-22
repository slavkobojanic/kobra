# Huddle

A small team chat written in Kobra: sign in with a name, pick or create a room, and talk. Messages arrive live in every tab over a **channel**, survive a dropped connection, and are stored in SQLite (Postgres in production). Each room shows who is in it and who is typing, and the lobby shows how many messages you haven't read in each room, across every replica of the app. A room's creator moderates it, and a room may have a password.

It is a teaching example for channels ([`docs/design/44-channels.md`](../../docs/design/44-channels.md)), the typed, two-way connection between a browser tab and its server, and for Web Push ([`docs/design/50-push.md`](../../docs/design/50-push.md)): a room's **Notify me of new messages** tells you about new messages while no tab is open.

## Running it

| Command, at `examples/huddle` | What it does |
|---|---|
| `k dev` | Builds and serves the app on `http://localhost:8080`, with a SQLite file at `.k/huddle.db`, created and migrated at startup. Open it in two browsers (or a normal and a private window), sign in with two names and post in the same room. |
| `k test` | Runs the tests: two people in one room, a dropped and a forgotten connection, offline sends, unread counts, moderators, password rooms, and the client service against a scripted channel. |
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
| Per-message refusals as a typed error on `send` (`invalidText`, `notYourMessage`, `notModerator`, `cannotRemoveModerator`, …), with the channel staying open | `src/chat/ChatApi.k`, `src/chat/CommandError.k` |
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
| **A second channel**, the lobby's: every room with your unread count, then posts, deletions and your own reading as they happen, from a per-account and an all-rooms `Broadcast` merged into one stream | `src/chat/LobbyProtocol.k`, `src/chat/LobbyApi.k`, `src/chat/RoomActivity.k`, `src/chat/LobbyService.k`, `src/chat/Lobby.k` |
| **Unread counts**: where each person stopped reading, per room, stored in `Membership.lastRead`; the room page marks what it shows read while the tab is visible (`PageVisibility`) | `src/chat/Membership.k`, `src/chat/RoomService.k`, `src/chat/Room.k` |
| **Moderators**: a room's creator moderates it, and may make others moderators, delete anyone's message and remove a member; each action is checked on the server, against the database, for every command | `src/chat/ChatApi.k`, `src/chat/Room.k` |
| **Password rooms**: an Argon2id hash (`k.crypto.Password`), asked for once per person (`unlock`, which remembers it), five tries a minute (`RateLimit`) | `src/chat/ChatProtocol.k`, `src/chat/ChatApi.k`, `src/chat/Room.k` |
| Message numbers from a per-room counter taken in the posting transaction, so a number is never reused after a deletion | `src/chat/ChatRoomRepository.k` |
| Two-person tests with `TestPeers`, connection tests with `TestChannels` and `TestConnectivity`, and a scripted `TestChannel` | `src/chat/Room.test.k`, `src/chat/RoomService.test.k` |

## How it works

### Joining a room

1. The `Room` page starts `RoomService.follow(room:)` from an `@Effect(on: [name])`. It runs in the browser only; leaving the page cancels it, which closes the channel.
2. `follow` calls `chat.room(name:)`. That one call opens the channel on the tab's WebSocket (`/_k/channels`, shared by every channel of the tab). The protocol's `Authenticated` guard and the handler run on the server; a refusal, such as `RoomError.noSuchRoom` for `/room/nowhere`, is thrown by the call itself.
3. `ChatApi.room(name:)` checks the person may join (a room with a password needs `unlock` first; someone a moderator removed can't come back), makes them a member the first time, and subscribes to the room's `Broadcast` **before** reading the stored messages, so a message posted in between arrives after the snapshot instead of being lost. It returns `Channel(sending:)` with the snapshot first (then the room's moderators, if it has any), and a receiving function.
4. `RoomService` reads `opened.messages` with `for await`, and applies each event. Applying is idempotent: a message that arrives in a snapshot and again as an echo is shown once, in the order of its `number`.

### Posting

1. `RoomService.post(text:)` adds a `PendingMessage` with a random client id, so the message shows at once as **Sending…**.
2. `channel.send(.post(text:clientId:))` waits for the server's acknowledgement. On the server the receiving function runs once per message, in order, inside one channel's request: it checks the text, takes the room's next number, stores the message in a transaction and publishes `.posted(message:clientId:)` to the room and `.posted(room:number:author:)` to the lobbies, both delivered when the transaction commits.
3. Every channel of the room receives the echo, the sender's included. The sender replaces its pending copy, matched by the client id.
4. A refused message (`invalidText`) throws from `send`; the page puts the text back in the field and says why. The channel stays open.

### Who is here

1. `ChatApi.room(name:)` watches `RoomPresence` and merges its changes into the channel's stream (`Stream.merge`), **then** joins the room with `Member(name:)` and reads who is present for the snapshot, so nobody arriving in between is missed.
2. The lease belongs to the channel's request: when the channel closes (the page is left, the session ends, or a dropped connection doesn't come back within 30 seconds), the person leaves, with no cleanup code. A resumed channel never left, so a blip doesn't flicker.
3. Ana in two tabs holds two leases of the same `Member`: the room sees one `joined` when the first opens and one `left` when the last closes.
4. `RoomService` keeps `present` from the snapshot and the `joined`, `left` and `updated` events; the page lists it as "In this room".

**Typing** isn't presence. The page sends `ChatCommand.typing` while the draft changes, at most once every 3 seconds; the server passes `ChatEvent.typing(name:)` to the room; `RoomService` shows a name until 5 seconds after its last event, or until that person's message arrives. Nothing is stored, so nothing needs cleaning up.

### The lobby and unread counts

The lobby (`/`) is a channel too, `LobbyProtocol.lobby()`, opened from the page's `@Effect`. Its handler subscribes to `RoomActivity` (everyone's posts and deletions, and the person's own reading) and sends a snapshot: every room, and for each room the person is a member of, the number of the newest message they have seen (`lastRead`) and how many messages by others came after it.

1. **Seen** means shown in a visible tab. After applying each event, `RoomService` sends `ChatCommand.read(upTo:)` with the newest message's number, once per new one, while `PageVisibility.isVisible`; the page sends it again when the tab is shown. The server moves `Membership.lastRead` forward (never back, in one conditional `UPDATE`), counts what is left, and tells the person's lobbies `.read(room:upTo:unread:)`.
2. **The browser keeps the count** from then on: a post by someone else with a number after `lastRead` adds one, its deletion takes one away, and a `read` replaces the count with the server's. Comparing numbers with `lastRead` makes the order of events from different replicas not matter: a post that arrives after the read that covers it is ignored.
3. **Only members hear a room's activity**: the lobby's stream passes on a room's posts and deletions only while the person is a member of it, asked of the database for each one by an async `filter`, so a password room's messages reach nobody else, not even as a count, and a removal made through another replica counts at once.

The lobby's form creates a room (`LobbyCommand.create(name:password:)`, refused with `LobbyError.invalidName` or `.nameTaken`), and every lobby lists it at once. The three rooms that came with the app are rows of `chat_room`, inserted by migration `0003`, without a creator.

### Moderators

A room's creator is its first moderator (`Membership.moderator`). The room's channel sends `.moderators(names:)` after the snapshot and again when they change; the page shows "(moderator)" beside them, and gives a moderator a **Delete** button on every message and **Make … a moderator** and **Remove …** beside everyone else in the room.

Every moderator action is a command, checked on the server against the database each time (so a change made through another replica counts at once), and refused with a typed error over the channel: `notModerator`, `noSuchMember`, `cannotRemoveModerator`, and `notYourMessage` for someone else's message. A deletion fans out as `.deleted(id:)` to everyone in the room and as `.deleted(room:number:author:)` to the lobbies.

**Removing a member** marks their membership removed, deletes their push subscriptions for the room, and publishes `.removed(name:)`. Their own channel hears that last: its sending stream passes nothing after it, their presence lease is left at once, and any further command is refused `removedFromRoom`. Their tab closes the channel and says "A moderator removed you from #team."; opening the room again throws `RoomError.removed`.

### Password rooms

A room created with a password stores its Argon2id hash (`Password.hash(password:)`), never the password. Opening it throws `RoomError.passwordRequired` until the person gives the password once: `ChatProtocol.unlock(room:password:)` checks it with `Password.verify(password:hash:)` and stores a membership, which is what the server remembers. A wrong password is `RoomError.wrongPassword`; the rpc `requires RateLimit(5.perMinute)`, so a sixth try within a minute is `rpcFailure(.rejected(.rateLimited(retryAfter:)))`, which the page shows as "Too many tries. Wait a minute, then try again." Nobody who hasn't given the password reads the room, follows it for push notifications, or hears its activity in the lobby.

### Reconnecting

The runtime owns the connection; the page only shows `RoomService.banner`, which is derived from `channel.state`.

- **A short drop** (a laptop lid, a train tunnel) resumes within 30 seconds: the server kept the channel, replays what the tab missed and skips what it already handled. Nothing is lost or handled twice.
- **A long drop or a restarted server** restarts the channel: the guard and the handler run again, and the new snapshot brings the room up to date. The lobby's channel does the same, so its counts are right again after a restart.
- **A message sent while disconnected** waits at most 10 seconds. If it couldn't be delivered, it stays in the list marked **Not sent.** with a **Retry** button, so the person decides whether to send it again: after a restart the runtime can't know whether the server handled it.

### Push notifications

A room's **Notify me of new messages** (`AlertSwitch`, rendered in the browser only) calls `RoomAlerts.turnOn(room:)` from the click: `PushSubscriptions.requestAccess()` shows the notification prompt the first time, the server's key comes from `AlertsProtocol.serverKey()` (the public half of `K_PUSH_PRIVATE_KEY`), `subscribe(serverKey:)` subscribes the browser, and `follow(room:subscription:)` stores a `RoomAlert` for the signed-in person, the room and the browser's endpoint. **Stop notifying me** deletes it.

Posting a message enqueues a `RoomAlertJob` with the message, in the transaction that stores it. The job pushes `"ana in #general"` and the text to the room's followers, except the author and anyone in the room right now (`RoomPresence`), with the tag `room-<name>`, so one notification per room stays on screen, and `opens: .room(name:)`, so a click opens the room. A subscription the push service says is gone (`WebPushError.expired`) is deleted; an unavailable push service retries the job with backoff.

The browser's push service (Google's for Chrome, Mozilla's for Firefox, Apple's for Safari) carries the message, encrypted so it can't read it; the app is identified by its own key, with no account. In production set `K_PUSH_PRIVATE_KEY_FILE` to a file holding the key.

## Two replicas

`docker-compose.yml` runs Postgres, two replicas of the app and nginx with plain round robin; `deploy/nginx.conf` passes the WebSocket upgrade (`proxy_http_version 1.1`, `Upgrade`, `Connection "upgrade"`, `proxy_read_timeout 75s`). Nothing needs configuring for the replicas to talk:

- every `Broadcast` publish is delivered on its own replica and sent to the others with Postgres `NOTIFY`, so a message posted through `app1` reaches a tab connected to `app2`;
- presence leases live in the `k_presence` table, renewed by their replica every 10 seconds; a crashed replica's members leave 30 seconds later;
- unread counts are computed from the shared database, and the lobby's events cross replicas like the rooms' (`examples.rs` checks that reading a room through one replica updates the lobby open on the other);
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
  0003_create_rooms_and_memberships.k   the rooms (the three there were, inserted) and memberships
src/
  accounts/              module accounts: signing in and out
    AccountId.k            struct: the session user's id, the name they chose
    AuthProtocol.k         protocol: signIn(name:), signOut()
    AuthApi.k              server implementation, using SessionManager
    AuthService.k          client service; exposes the session user
    SignInError.k          enum
    SignIn.k               page
  chat/                  module chat: rooms and messages
    ChatProtocol.k         protocol: the room(name:) channel rpc, unlock(room:password:)
    ChatApi.k              server implementation: the handler, its receiving function, moderation
    ChatCommand.k          enum: what the browser sends (C)
    ChatEvent.k            enum: what the server sends (S)
    Member.k               struct: someone in a room
    RoomPresence.k         @Scope(.server) Presence of each room's people
    CommandError.k         enum: why a message was refused (E)
    RoomError.k            enum: why a room can't be joined
    RoomEvents.k           @Scope(.server) Broadcast of each room's events
    ChatRoom.k             entity: a room, its creator, its password's hash, its last message number
    ChatRoomRepository.k   repository
    Membership.k           entity: someone in a room: where they stopped reading, moderator, removed
    MembershipRepository.k repository
    Message.k              entity (server-only)
    MessageRepository.k    repository
    ChatMessage.k          struct: a message as the browser sees it
    PendingMessage.k       struct: a message on its way
    RoomNames.k            functions: a typed room name as stored, and the rule for one
    RoomService.k          client service: follows a room, optimistic sends, reads, moderation
    LobbyProtocol.k        protocol: the lobby() channel rpc
    LobbyApi.k             server implementation: the rooms with their counts, creating a room
    LobbyCommand.k         enum: what the lobby sends (C)
    LobbyEvent.k           enum: what the lobby hears (S)
    LobbyError.k           enum: why a room couldn't be created (E)
    RoomSummary.k          struct: a room as the lobby shows it
    RoomActivity.k         @Scope(.server) Broadcast of what changes the lobby's counts
    LobbyService.k         client service: follows the lobby, keeps the counts
    Lobby.k                page at "/": the rooms with their unread counts, and a form to create one
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

- **`TestPeers`** opens a second browser with its own session against the same server, database and clock: `let bob = await peers.signIn(userId: AccountId(name: "bob"))`, then `let chat: ChatProtocol = bob.client()`. `Room.test.k` uses it for "a message posted by one person appears for another", "only a message's author can delete it", presence ("the people in the room are listed, and leave when their channel closes") and typing; `Lobby.test.k` for unread counts (rising live, cleared by reading in another tab, a deletion, a hidden tab, a forgotten channel) and creating rooms; `Moderation.test.k` for every moderator action and refusal, and a removed member; `PasswordRooms.test.k` for the hash, asking once, the rate limit and keeping a room's activity to its members.
- **`TestChannels`** controls the test's own connection: `network.drop()` drops it (it resumes after its backoff on the `TestClock`), `network.drop(resumable: false)` makes the server forget it (it restarts with a new snapshot), and `network.open` lists the open channels.
- **`TestConnectivity.goOffline()`** drops the connection and stops reconnecting until `goOnline()`, and **`TestPageVisibility.hide()`** hides the tab, so its messages stay unread until `show()`.
- **`TestPushSubscriptions`** and **`RecordingWebPush`** replace the browser's push subscription and the server's sender: `AlertSwitch.test.k` turns a room's notifications on and off and checks that a posted message is handed to the job (`RecordingJobQueue`), and `RoomAlertJob.test.k` runs the job: the author and the people in the room aren't told, an expired subscription is deleted, an outage is retried, a server without a key fails the job.
- **`TestChannel<C, S, E>`** is a scripted channel for testing `RoomService` without a server: `RoomService.test.k` delivers events, refuses the next message, drops and closes it.

Outside `k test`: `crates/k_cli/tests/examples.rs` runs two replicas on Postgres (a message, presence and an unread count cross between them), and `crates/k_cli/tests/chrome.rs`' `huddle_counts_unread_live_and_a_moderator_deletes_for_everyone_in_chrome` runs `k dev` with two real Chromes, one per person: Ana creates a room, Bob's lobby counts her message the moment she posts it, and when Ana deletes Bob's message as the room's moderator it vanishes from his screen.

## Design documents used

- [44-channels.md](../../docs/design/44-channels.md): channels, reconnecting, limits and testing
- [29-server-streaming.md](../../docs/design/29-server-streaming.md): `Broadcast` and snapshots
- [38-deployment.md](../../docs/design/38-deployment.md): `dist/app`, images and replicas
- [26-auth-and-sessions.md](../../docs/design/26-auth-and-sessions.md): `SessionManager` and guards, `RateLimit`
- [33-crypto.md](../../docs/design/33-crypto.md): `Password` (Argon2id)
- [19-persistence.md](../../docs/design/19-persistence.md): unique fields, bulk updates, data migrations
- [21-testing.md](../../docs/design/21-testing.md): tests on the real stack in-process
- [50-push.md](../../docs/design/50-push.md): `PushSubscriptions`, `WebPush` and the service worker
- [30-background-jobs.md](../../docs/design/30-background-jobs.md): `RoomAlertJob`
