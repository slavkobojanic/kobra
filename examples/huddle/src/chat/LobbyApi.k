import k.auth.Session
import k.crypto.Password
import k.crypto.Uuid
import k.rpc.Channel
import k.rpc.RpcOnlyError

/// The server side of `LobbyProtocol`.
public service LobbyApi implements LobbyProtocol {
    @Inject let rooms: ChatRoomRepository
    @Inject let memberships: MembershipRepository
    @Inject let messages: MessageRepository
    @Inject let activity: RoomActivity
    @Inject let session: Session

    rpc lobby() async throws RpcOnlyError -> Channel<LobbyCommand, LobbyEvent, LobbyError> {
        let me = session.user.id.name

        // Subscribe before reading, as for a room.
        let live = activity.subscribe(member: me).filter { event => await passes(event: event, me: me) }
        let mine = await memberships.of(member: me)
        var summaries: List<RoomSummary> = []

        for room in await rooms.all() {
            if let membership = mine.firstWhere({ m => m.room == room.name and not m.removed }) {
                summaries.append(RoomSummary(name: room.name, locked: room.passwordHash != nil, lastRead: membership.lastRead, unread: await messages.unread(room: room.name, after: membership.lastRead, for: me)))
            } else {
                summaries.append(RoomSummary(name: room.name, locked: room.passwordHash != nil, lastRead: 0, unread: nil))
            }
        }

        return Channel(sending: live.startingWith(.snapshot(rooms: summaries))) { command =>
            try await handle(command: command, creator: me)
        }
    }

    /// Whether `event` reaches `me`'s lobby: a room's posts and deletions
    /// only while they are a member of it, read from the database for each
    /// one, so a room's activity never reaches anyone who couldn't open
    /// it, and a removal on any replica counts at once. A `read` is the
    /// person's own.
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

    private func handle(command: LobbyCommand, creator: String) async throws LobbyError {
        match command {
            case .create(let typed, let password) {
                let name = roomName(typed: typed)

                guard isValidRoomName(name) else {
                    throw LobbyError.invalidName
                }

                var hash: String? = nil

                if let password, password.isNotEmpty {
                    hash = await Password.hash(password: password)
                }

                do {
                    try await rooms.insert(ChatRoom(id: Uuid.random(), name: name, createdBy: creator, passwordHash: hash, lastNumber: 0))
                } catch .name {
                    throw LobbyError.nameTaken
                }

                // The creator is in, password or not, as its moderator.
                await memberships.join(room: name, member: creator, moderator: true)
                activity.announce(event: .created(room: RoomSummary(name: name, locked: hash != nil, lastRead: 0, unread: nil)))
            }
        }
    }
}
