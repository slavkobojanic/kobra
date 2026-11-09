import k.auth.Session
import k.crypto.Uuid
import k.db.Database
import k.rpc.Channel
import k.time.Clock

/// The server side of `ChatProtocol`.
///
/// The protocol's `Authenticated` guard runs before every rpc, and for a
/// channel when it opens, so `session.user` is never `nil` here, in the
/// handler and in its receiving function.
public service ChatApi implements ChatProtocol {
    @Inject let messages: MessageRepository
    @Inject let events: RoomEvents
    @Inject let session: Session
    @Inject let database: Database
    @Inject let clock: Clock

    rpc room(name: String) async throws RoomError -> Channel<ChatCommand, ChatEvent, CommandError> {
        guard Rooms.names.contains(name) else {
            throw RoomError.noSuchRoom
        }

        // Subscribe before reading, so a message posted in between arrives
        // after the snapshot instead of being lost.
        let live = events.subscribe(room: name)
        let stored = await messages.all(room: name)
        let me = session.user.id.name

        return Channel(sending: live.startingWith(.snapshot(messages: stored.map(chatMessage)))) { command =>
            try await handle(command: command, room: name, author: me)
        }
    }

    /// Runs once per command, in order, one at a time per channel, for the
    /// person who opened it.
    private func handle(command: ChatCommand, room: String, author: String) async throws CommandError {
        match command {
            case .post(let text, let clientId) {
                let trimmed = text.trimmed()

                guard trimmed.isNotEmpty and trimmed.count <= 500 else {
                    throw CommandError.invalidText
                }

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
            }

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
        }
    }
}

/// The browser's view of a stored message.
func chatMessage(message: Message) -> ChatMessage {
    return ChatMessage(
        id: message.id,
        number: message.number,
        author: message.author,
        text: message.text,
        sentAt: message.sentAt
    )
}
