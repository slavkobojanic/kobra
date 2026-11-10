import k.auth.Session
import k.rpc.Channel
import k.rpc.RpcOnlyError

/// The rooms, followed live from the browser, with this person's unread
/// count in each.
///
/// The server sends each room's count once, in the snapshot; after that
/// the browser keeps it from the posts and deletions of the rooms this
/// person joined, and takes the server's count again whenever they read a
/// room, in any tab. Comparing message numbers with `lastRead` makes the
/// order of those events not matter, even when they come from different
/// replicas.
public service LobbyService {
    @Inject let lobby: LobbyProtocol
    @Inject let session: Session

    /// Every room, by name.
    @State var rooms: List<RoomSummary> = []

    @State var channel: Channel<LobbyCommand, LobbyEvent, LobbyError>? = nil

    /// Follows the lobby until the caller is cancelled (the page is left).
    func follow() async throws RpcOnlyError {
        let opened = try await lobby.lobby()

        channel = opened

        for await event in opened.messages {
            apply(event: event)
        }
    }

    /// Creates the room `name`, with a password unless `password` is
    /// empty.
    ///
    /// - Throws: `LobbyError.invalidName`, `.nameTaken`, `.rpcFailure`.
    func create(name: String, password: String) async throws LobbyError {
        guard let channel else {
            throw LobbyError.rpcFailure(.offline)
        }

        try await channel.send(.create(name: name, password: if password.isEmpty { nil } else { password }))
    }

    /// Applies one event to `rooms`.
    func apply(event: LobbyEvent) {
        match event {
            case .snapshot(let all) {
                rooms = all
            }

            case .created(let room) {
                if not rooms.any({ r => r.name == room.name }) {
                    rooms = (rooms + [room]).sortedBy { r => r.name }
                }
            }

            case .posted(let room, let number, let author) {
                count(room: room, number: number, author: author, change: 1)
            }

            case .deleted(let room, let number, let author) {
                count(room: room, number: number, author: author, change: -1)
            }

            case .read(let room, let upTo, let unread) {
                rooms = rooms.map { r => if r.name == room { r.copy(lastRead: upTo, unread: unread) } else { r } }
            }
        }
    }

    /// Adds `change` to the count of `room` for message `number`, unless
    /// this person wrote it or has read past it.
    private func count(room: String, number: Int, author: String, change: Int) {
        if author == session.user?.id.name {
            return
        }

        rooms = rooms.map { r => counted(r, room: room, number: number, change: change) }
    }

    private func counted(_ r: RoomSummary, room: String, number: Int, change: Int) -> RoomSummary {
        guard let unread = r.unread, r.name == room and number > r.lastRead else {
            return r
        }

        return r.copy(unread: unread + change)
    }
}
