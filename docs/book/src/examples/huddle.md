# Huddle: a team chat on channels

`examples/huddle` is a small team chat: sign in with a name, pick or create a room, and talk. Messages
arrive live in every tab over a [channel](../apps/channels.md), survive a dropped connection, and are
stored in SQLite in development and Postgres in production. Each room shows who is in it and who is
typing, and the lobby how many messages you haven't read in each room. A room's creator moderates it,
and a room may have a password.
A room's **Notify me of new messages** sends a [push notification](../apps/notifications.md#push-messages-from-the-server)
for each new message while you're away. It is the example of channels and of Web Push; its README walks
through every file.

```text
huddle/
├── App.k, k.toml
├── Dockerfile, docker-compose.yml, deploy/nginx.conf
├── migrations/0001_create_messages.k, 0002_create_room_alerts.k, 0003_create_rooms_and_memberships.k
└── src/
    ├── accounts/   signing in with a name: AuthProtocol, AuthApi, AuthService, SignIn
    ├── chat/       the room channel: ChatProtocol, ChatApi, RoomEvents, RoomPresence, RoomService, Room;
    │               the lobby channel: LobbyProtocol, LobbyApi, RoomActivity, LobbyService, Lobby;
    │               rooms and members: ChatRoom, Membership and their repositories;
    │               push: AlertsProtocol, AlertsApi, RoomAlert, RoomAlertJob, RoomAlerts, AlertSwitch
    └── shell/      AppLayout, HuddleTheme
```

## Running it

| Command, at `examples/huddle` | What it does |
|---|---|
| `k dev` | Builds and serves the app on port 8080 with a SQLite file at `.k/huddle.db`. Open it in two browsers (or a normal and a private window), sign in with two names, and post in the same room. |
| `k test` | Runs the tests, two people in one room included, all in-process. |
| `k build` | Builds `dist/`, whose `dist/app` serves the pages, the rpcs and the channels' WebSocket. |
| `docker compose up --build --wait` | Runs Postgres, two replicas and nginx on `http://localhost:8080` ([below](#two-replicas)). |

To see a reconnect, stop `k dev` while a room is open and start it again: the page shows
**Reconnecting…**, then catches up from the new snapshot.

## The room channel

One rpc opens a room. Its result says what each side sends and why a message can be refused:

```k
// examples/huddle/src/chat/ChatProtocol.k
    rpc room(name: String) throws RoomError -> Channel<ChatCommand, ChatEvent, CommandError>
```

```k
// examples/huddle/src/chat/ChatCommand.k
public enum ChatCommand implements Codable, Equatable {
    /// Posts `text`. `clientId` comes back with the echo, so the sender
    /// replaces its pending copy with the stored message.
    case post(text: String, clientId: Uuid)
    /// Deletes one of the sender's own messages, or, from a moderator,
    /// anyone's.
    case delete(id: Uuid)
    /// The sender is typing. Nothing stores it: it is passed on to the room.
    case typing
    /// The sender has seen the room's messages up to number `upTo`.
    case read(upTo: Int)
    /// A moderator makes the member `name` a moderator too.
    case promote(name: String)
    /// A moderator removes the member `name` from the room for good.
    case remove(name: String)
}
```

`ChatApi.room(name:)` subscribes to the room's `Broadcast` before reading the stored messages, sends
the snapshot first, and handles each command in order. A post is stored and published inside one
transaction, so its echo (and the lobbies' count of it, [below](#unread-counts-a-second-channel)) goes out
only once the message is stored, and so does the job that pushes it to the room's followers
([below](#push-notifications)):

```k
// examples/huddle/src/chat/ChatApi.k
                await database.transaction {
                    let message = Message(
                        id: Uuid.random(),
                        room: room,
                        number: await rooms.nextNumber(room: room),
                        author: author,
                        text: trimmed,
                        sentAt: clock.now
                    )

                    await messages.insert(message)

                    // Held until the commit, so nobody sees a message that
                    // wasn't stored.
                    events.publish(room: room, event: .posted(message: chatMessage(message: message), clientId: clientId))
                    activity.announce(event: .posted(room: room, number: message.number, author: author))

                    // Pushed to the room's followers once it commits.
                    await jobs.enqueue(RoomAlertJob.kind, payload: NewMessageAlert(room: room, author: author, text: trimmed))
                }
```

## Who is here, and who is typing

`RoomPresence` holds a `Presence<String, Member>`. The handler watches it, joins the room for as long as
the channel is open, and puts who is present in the snapshot; `RoomService` applies `joined`, `left`
and `updated` to its `present` list, which the page shows as "In this room". Ana's two tabs are one
entry, and closing the last one removes her
([Presence](../apps/channels.md#presence-who-is-here)).

Typing is an ordinary command and event. The page sends `typing` when the draft changes, at most once
every 3 seconds, and shows "bob is typing…" until 5 seconds after Bob's last one, on the injected
`Clock`:

```k
// examples/huddle/src/chat/RoomService.k
            case .typing(let name) {
                // Shown for 5 seconds after the last one, on the injected
                // clock, so a test advances it.
                typingTimers.get(name)?.cancel()

                if not typing.contains(name) {
                    typing.append(name)
                }

                typingTimers.set(name, clock.after(5.seconds) {
                    stopTyping(name: name)
                })
            }
```

## Optimistic sends

`RoomService.post(text:)` shows the message at once as pending, under a client id it chose. The echo
carries the id back and replaces it. A message that couldn't be delivered stays, marked failed, and
the page offers **Retry**: after a restart the runtime can't know whether the server handled it, so
the person decides.

```k
// examples/huddle/src/chat/RoomService.k
        do {
            try await channel.send(.post(text: message.text, clientId: message.clientId))
        } catch .rpcFailure(let failure) {
            // Undelivered, or delivered and not acknowledged before a
            // restart: kept, so the person decides whether to send it again.
            replace(clientId: message.clientId, with: PendingMessage(clientId: message.clientId, text: message.text, failed: true))

            throw CommandError.rpcFailure(failure)
```

## Unread counts: a second channel

The lobby is a channel of its own, `LobbyProtocol.lobby()`. Its snapshot lists every room with, for
each room you joined, where you stopped reading (`lastRead`, stored per person and room in
`Membership`) and how many messages by others came after it. Then it passes on posts and deletions in
those rooms, and your own reading in any tab, from two `Broadcast` keys merged into one stream:

```k
// examples/huddle/src/chat/RoomActivity.k
    func subscribe(member: String) -> Stream<LobbyEvent> {
        return Stream.merge(events.subscribe(key: ""), events.subscribe(key: member))
    }
```

A room you read is marked read by its page: after each event `RoomService` sends `read(upTo:)` with the
newest message's number while the tab is visible (`PageVisibility`), and the server answers your
lobbies with the count that is left. The browser keeps the count in between, and compares message
numbers with `lastRead`, so a post and the read that covers it can arrive in either order, from either
replica:

```k
// examples/huddle/src/chat/LobbyService.k
    private func counted(_ r: RoomSummary, room: String, number: Int, change: Int) -> RoomSummary {
        guard let unread = r.unread, r.name == room and number > r.lastRead else {
            return r
        }

        return r.copy(unread: unread + change)
    }
```

Counts are kept only while the lobby's channel is open, so each visit to the lobby starts from an empty
list and shows the counts of its own snapshot, never the ones a previous visit left behind.

Numbers come from a counter on the room, taken in the transaction that stores the message, so a number
is never reused after a deletion. The lobby's channel passes on only the rooms you are a member of, so a
password room's activity reaches nobody else. Its stream's `filter` is async and asks the database for
each post, so a removal made through another replica counts at once:

```k
// examples/huddle/src/chat/LobbyApi.k
    private func passes(event: LobbyEvent, me: String) async -> Bool {
        match event {
            case .posted(let room, _, _), .deleted(let room, _, _) {
                return await memberships.isMember(room: room, member: me)
            }
            case .snapshot, .created, .read {
                return true
            }
        }
    }
```

Joining is `MembershipRepository.join(room:member:moderator:)`, which catches its own table's unique
violation (`catch .key`): a second tab joining at the same moment keeps the first membership.

## Moderators and password rooms

The lobby's form creates a room (`LobbyCommand.create(name:password:)`); its creator is its first
moderator. A moderator deletes anyone's message, makes another member a moderator and removes a member,
each a command on the room's channel that the server checks against the database every time and
refuses with a typed error that leaves the channel open:

```k
// examples/huddle/src/chat/ChatApi.k
            case .promote(let name) {
                guard await isModerator(room: room, name: author) else {
                    throw CommandError.notModerator
                }
```

A removed member's channel hears `.removed(name:)` last: the server's stream ends there
(`prefix(through:)`), which closes the channel `byServer` and ends their presence lease at once, and
opening the room again throws `RoomError.removed`.

A room with a password stores its Argon2id hash (`Password.hash(password:)`). Opening it throws
`RoomError.passwordRequired` until you give the password once to `unlock(room:password:)`, which
stores your membership, so the server remembers. Wrong passwords are limited with the ordinary guard:

```k
// examples/huddle/src/chat/ChatProtocol.k
    rpc unlock(room: String, password: String) throws RoomError requires RateLimit(5.perMinute)
```

## Push notifications

A room's **Notify me of new messages** subscribes the browser with `PushSubscriptions` and stores the
subscription for the signed-in person and the room (`RoomAlert`, through `AlertsProtocol`). Posting a
message enqueues a `RoomAlertJob` in the transaction that stores it, and the job pushes the message with
`WebPush` to the room's other followers who aren't in the room right now. A click on the notification
opens the room. One notification per room stays on screen, since each has the tag `room-<name>`, and a
subscription the push service says is gone is deleted.

The switch shows only when the server has a VAPID key. To try it, make one and put it in `.env`:

```bash
k keys new p256-ecdsa      # prints the private JWK, then the public one
```

```text
K_PUSH_PRIVATE_KEY={"kty":"EC","crv":"P-256",…}
K_PUSH_CONTACT=mailto:you@example.com
```

Your browser's push service delivers the message, so this needs a browser that can reach it (Chrome's,
Firefox's or Apple's); no account is involved. `.env` is read only in development; a deployment sets
the same variables, the key preferably as a file with `K_PUSH_PRIVATE_KEY_FILE`.

## Two replicas

`docker-compose.yml` runs Huddle as in production, with no account anywhere: Postgres, two replicas of
the app (`app1` and `app2`), and nginx in front with plain round robin. `deploy/nginx.conf` passes the
WebSocket upgrade and allows idle sockets 75 seconds, above the 25-second heartbeat:

```text
proxy_http_version 1.1;
proxy_set_header Upgrade $http_upgrade;
proxy_set_header Connection "upgrade";
proxy_read_timeout 75s;
```

Two people whose tabs land on different replicas still see each other's messages, presence,
typing and unread counts: every `Broadcast` and `Presence` crosses replicas over Postgres
([Several replicas](../apps/channels.md#several-replicas)). `docker compose restart app1` shows the
other side: the tabs on `app1` show **Reconnecting…**, reconnect through nginx to `app2`, and restart
their channels there with a fresh snapshot.

## Tests

`src/chat/Room.test.k` signs a second person in with `TestPeers` and checks that what Bob posts
appears on Ana's screen, that Bob can't delete Ana's message, that Bob is listed once in "In this room"
with two tabs open and leaves with the last, that his typing shows for 5 seconds, and that a guest
can't open a room. It
drops the connection with `TestChannels` (the banner shows, then the missed message arrives once), makes
the server forget it (a new snapshot), and takes the browser offline with `TestConnectivity` (a message
marked **Not sent.**, then retried). `src/chat/RoomService.test.k` tests `RoomService` alone against a
scripted `TestChannel`. `src/chat/AlertSwitch.test.k` turns a room's notifications on and off with
`TestPushSubscriptions` and checks that a posted message is handed to the job, and
`src/chat/RoomAlertJob.test.k` runs the job against `RecordingWebPush`: the author and the people in the
room aren't told, an expired subscription is deleted, an outage is retried. `src/chat/Lobby.test.k`
checks unread counts (rising live as Bob posts, cleared by reading in another tab, a deletion, a hidden
tab, a forgotten channel) and creating rooms; `src/chat/Moderation.test.k` every moderator action and
refusal; `src/chat/PasswordRooms.test.k` the hash, asking once, the rate limit and that a room's
activity stays with its members.

Outside `k test`, a real-Chrome test runs two browsers, one per person: Bob's lobby counts Ana's
message the moment she posts it, and a message Ana deletes as the room's moderator vanishes from Bob's
screen.
