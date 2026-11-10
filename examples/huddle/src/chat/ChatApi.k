import k.auth.Session
import k.crypto.Uuid
import k.db.Database
import k.rpc.Channel
import k.server.JobQueue
import k.time.Clock

/// The server side of `ChatProtocol`.
///
/// The protocol's `Authenticated` guard runs before every rpc, and for a
/// channel when it opens, so `session.user` is never `nil` here, in the
/// handler and in its receiving function.
public service ChatApi implements ChatProtocol {
    @Inject let messages: MessageRepository
    @Inject let events: RoomEvents
    @Inject let presence: RoomPresence
    @Inject let session: Session
    @Inject let database: Database
    @Inject let clock: Clock
    @Inject let jobs: JobQueue

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

                    // Pushed to the room's followers once it commits.
                    await jobs.enqueue(RoomAlertJob.kind, payload: NewMessageAlert(room: room, author: author, text: trimmed))
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

            case .typing {
                events.publish(room: room, event: .typing(name: author))
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
