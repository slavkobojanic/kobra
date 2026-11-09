import k.auth.Session
import k.auth.SessionManager
import k.db.Database
import k.rpc.Channel
import k.rpc.RpcOnlyError

/// The server side of `ChatProtocol`.
public service ChatApi implements ChatProtocol {
    @Inject let events: RoomEvents
    @Inject let store: NoteRepository
    @Inject let database: Database
    @Inject let sessions: SessionManager
    @Inject let session: Session

    /// Messages this channel handled: one per open channel, since the
    /// channel keeps its request container.
    private var handled: Int = 0

    rpc room(id: String) async throws RoomError -> Channel<ChatCommand, ChatEvent, CommandError> {
        guard id != "closed" else {
            throw RoomError.closed
        }

        let live = events.subscribe(room: id)
        let stored = await store.texts()

        return Channel(sending: live.startingWith(ChatEvent.snapshot(notes: stored))) { command =>
            try await handle(command: command, room: id)
        }
    }

    rpc pair(first: String, second: String) async throws RoomError -> Channel<ChatCommand, ChatEvent, CommandError> {
        let live = Stream.merge(events.subscribe(room: first), events.subscribe(room: second))

        return Channel(sending: live) { command =>
            try await handle(command: command, room: first)
        }
    }

    rpc members(id: String) async throws RoomError -> Channel<ChatCommand, ChatEvent, CommandError> {
        let live = events.subscribe(room: "members-{id}")

        return Channel(sending: live.startingWith(ChatEvent.user(name: session.user.id.name))) { command =>
            try await handle(command: command, room: "members-{id}")
        }
    }

    rpc announce(room: String, text: String) async throws RpcOnlyError {
        events.publish(room: room, event: ChatEvent.posted(text: text))
    }

    rpc notes() async throws RpcOnlyError -> List<String> {
        return await store.texts()
    }

    rpc signIn(name: String) async throws RpcOnlyError {
        await sessions.signIn(userId: AccountId(name: name))
    }

    rpc signOut() async throws RpcOnlyError {
        await sessions.signOut()
    }

    // Runs once per client message, in order, one at a time per channel.
    private func handle(command: ChatCommand, room: String) async throws CommandError {
        handled = handled + 1

        match command {
            case .post(let text) {
                await database.transaction {
                    await store.add(text)
                }
                events.publish(room: room, event: ChatEvent.posted(text: text))
            }

            case .refuse {
                throw CommandError.notAllowed
            }

            case .count {
                events.publish(room: room, event: ChatEvent.count(n: handled))
            }

            case .whoami {
                events.publish(room: room, event: ChatEvent.user(name: session.user?.id.name ?? "guest"))
            }
        }
    }
}
