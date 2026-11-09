# Huddle: a team chat on channels

`examples/huddle` is a small team chat: sign in with a name, pick a room, and talk. Messages arrive
live in every tab over a [channel](../apps/channels.md), survive a dropped connection, and are stored
in SQLite. It is the example of channels; its README walks through every file.

```text
huddle/
├── App.k, k.toml
├── migrations/0001_create_messages.k
└── src/
    ├── accounts/   signing in with a name: AuthProtocol, AuthApi, AuthService, SignIn
    ├── chat/       the room channel: ChatProtocol, ChatApi, RoomEvents, RoomService, Lobby, Room
    └── shell/      AppLayout, HuddleTheme
```

## Running it

| Command, at `examples/huddle` | What it does |
|---|---|
| `k dev` | Builds and serves the app on port 8080 with a SQLite file at `.k/huddle.db`. Open it in two browsers (or a normal and a private window), sign in with two names, and post in the same room. |
| `k test` | Runs the tests, two people in one room included, all in-process. |
| `k build` | Builds `dist/`, whose `dist/app` serves the pages, the rpcs and the channels' WebSocket. |

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
    /// Deletes one of the sender's own messages.
    case delete(id: Uuid)
}
```

`ChatApi.room(name:)` subscribes to the room's `Broadcast` before reading the stored messages, sends
the snapshot first, and handles each command in order. A post is stored and published inside one
transaction, so its echo goes out only once the message is stored:

```k
// examples/huddle/src/chat/ChatApi.k
                await database.transaction {
                    let message = Message(
                        id: Uuid.random(),
                        room: room,
                        number: await messages.nextNumber(room: room),
                        author: author,
                        text: trimmed,
                        sentAt: clock.now
                    )

                    await messages.insert(message)

                    // Held until the commit, so nobody sees a message that
                    // wasn't stored.
                    events.publish(room: room, event: .posted(message: chatMessage(message: message), clientId: clientId))
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

## Tests

`src/chat/Room.test.k` signs a second person in with `TestPeers` and checks that what Bob posts
appears on Ana's screen, that Bob can't delete Ana's message, and that a guest can't open a room. It
drops the connection with `TestChannels` (the banner shows, then the missed message arrives once), makes
the server forget it (a new snapshot), and takes the browser offline with `TestConnectivity` (a message
marked **Not sent.**, then retried). `src/chat/RoomService.test.k` tests `RoomService` alone against a
scripted `TestChannel`.
