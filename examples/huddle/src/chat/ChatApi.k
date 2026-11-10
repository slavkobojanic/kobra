import k.auth.Session
import k.crypto.Password
import k.crypto.Uuid
import k.db.Database
import k.rpc.Channel
import k.rpc.PresenceLease
import k.server.JobQueue
import k.time.Clock

/// The server side of `ChatProtocol`.
///
/// The protocol's `Authenticated` guard runs before every rpc, and for a
/// channel when it opens, so `session.user` is never `nil` here, in the
/// handler and in its receiving function.
public service ChatApi implements ChatProtocol {
    @Inject let messages: MessageRepository
    @Inject let rooms: ChatRoomRepository
    @Inject let memberships: MembershipRepository
    @Inject let alerts: RoomAlertRepository
    @Inject let events: RoomEvents
    @Inject let activity: RoomActivity
    @Inject let presence: RoomPresence
    @Inject let session: Session
    @Inject let database: Database
    @Inject let clock: Clock
    @Inject let jobs: JobQueue

    /// This channel's place in the room's presence. Fields are per
    /// channel: the service lives as long as the channel's request.
    private var lease: PresenceLease<Member>? = nil

    /// `true` once a moderator removed this channel's person.
    private var removed: Bool = false

    rpc room(name: String) async throws RoomError -> Channel<ChatCommand, ChatEvent, CommandError> {
        guard let room = await rooms.find(name: name) else {
            throw RoomError.noSuchRoom
        }

        let me = session.user.id.name

        if let membership = await memberships.find(room: name, member: me) {
            guard not membership.removed else {
                throw RoomError.removed
            }
        } else {
            guard room.passwordHash == nil else {
                throw RoomError.passwordRequired
            }

            await join(room: name, member: me)
        }

        // Subscribe before reading, so a message posted or a person arriving
        // in between shows after the snapshot instead of being lost.
        let live = Stream.merge(
            events.subscribe(room: name),
            presence.watch(room: name).map { change => ChatEvent.presence(change) }
        ).filter { event => stillIn(after: event, me: me) }

        // In the room until this channel closes: leaving the page, a
        // dropped connection that doesn't come back, signing out, or being
        // removed.
        lease = presence.join(room: name, member: Member(name: me))

        let stored = await messages.all(room: name)
        let present = await presence.members(room: name)
        let moderators = await memberships.moderators(room: name)
        var sending = live

        if moderators.isNotEmpty {
            sending = live.startingWith(.moderators(names: moderators))
        }

        return Channel(sending: sending.startingWith(.snapshot(messages: stored.map(chatMessage), present: present))) { command =>
            try await handle(command: command, room: name, author: me)
        }
    }

    rpc unlock(room: String, password: String) async throws RoomError {
        guard let found = await rooms.find(name: room) else {
            throw RoomError.noSuchRoom
        }

        let me = session.user.id.name

        if let membership = await memberships.find(room: room, member: me) {
            guard not membership.removed else {
                throw RoomError.removed
            }

            return
        }

        if let hash = found.passwordHash {
            guard await Password.verify(password: password, hash: hash) else {
                throw RoomError.wrongPassword
            }
        }

        await join(room: room, member: me)
    }

    /// Whether `event` still reaches this channel: nothing does after the
    /// person's own removal, which also takes them out of the room's
    /// presence. The channel itself stays open until their tab closes it.
    private func stillIn(after event: ChatEvent, me: String) -> Bool {
        if removed {
            return false
        }

        if event == .removed(name: me) {
            removed = true
            lease?.leave()
        }

        return true
    }

    /// Runs once per command, in order, one at a time per channel, for the
    /// person who opened it.
    private func handle(command: ChatCommand, room: String, author: String) async throws CommandError {
        guard not removed else {
            throw CommandError.removedFromRoom
        }

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
            }

            case .delete(let id) {
                guard let message = await messages.find(id: id, room: room) else {
                    throw CommandError.messageGone
                }

                if message.author != author {
                    guard await isModerator(room: room, name: author) else {
                        throw CommandError.notYourMessage
                    }
                }

                await database.transaction {
                    await messages.delete(message)
                    events.publish(room: room, event: .deleted(id: id))
                    activity.announce(event: .deleted(room: room, number: message.number, author: message.author))
                }
            }

            case .typing {
                events.publish(room: room, event: .typing(name: author))
            }

            case .read(let upTo) {
                if await memberships.markRead(room: room, member: author, upTo: upTo) {
                    let unread = await messages.unread(room: room, after: upTo, for: author)

                    activity.tell(member: author, event: .read(room: room, upTo: upTo, unread: unread))
                }
            }

            case .promote(let name) {
                guard await isModerator(room: room, name: author) else {
                    throw CommandError.notModerator
                }

                guard await memberships.promote(room: room, member: name) else {
                    throw CommandError.noSuchMember
                }

                events.publish(room: room, event: .moderators(names: await memberships.moderators(room: room)))
            }

            case .remove(let name) {
                guard await isModerator(room: room, name: author) else {
                    throw CommandError.notModerator
                }

                guard let member = await memberships.find(room: room, member: name), not member.removed else {
                    throw CommandError.noSuchMember
                }

                guard not member.moderator else {
                    throw CommandError.cannotRemoveModerator
                }

                await database.transaction {
                    await memberships.remove(room: room, member: name)
                    // Nor told about the room's messages any more.
                    await alerts.deleteAll(room: room, member: name)
                    events.publish(room: room, event: .removed(name: name))
                }
            }
        }
    }

    /// Makes `member` a member of `room`. A second join (another tab, at
    /// the same moment) keeps the first.
    private func join(room: String, member: String) async {
        do {
            try await memberships.insert(memberships.new(room: room, member: member, moderator: false))
        } catch .key {
            // Already a member.
        }
    }

    /// Whether `name` moderates `room`, read for each command, so a change
    /// on any replica counts at once.
    private func isModerator(room: String, name: String) async -> Bool {
        return (await memberships.find(room: room, member: name))?.moderator ?? false
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
