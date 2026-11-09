import k.rpc.Channel
import k.rpc.ChannelState

/// Follows a room from the browser over a channel: what the `Room` page
/// shows, and what `crates/k_cli/tests/chrome.rs` drives in real Chrome.
public service RoomService {
    @Inject let chat: ChatProtocol

    /// The room's notes and events, oldest first.
    @State var lines: List<String> = []
    @State var channel: Channel<ChatCommand, ChatEvent, CommandError>? = nil

    /// What the status line shows.
    @Derived let connection: ChannelState = channel?.state ?? .connecting

    @Derived let status: String = match connection {
        case .connecting { "Connecting" }
        case .open { "Open" }
        case .reconnecting(let attempt, _) { "Reconnecting ({attempt})" }
        case .closed { "Closed" }
    }

    /// Follows `room` until the caller is cancelled or the server closes it.
    func follow(room: String) async throws RoomError {
        let opened = try await chat.room(id: room)
        channel = opened

        for await event in opened.messages {
            match event {
                case .snapshot(let notes) { lines = notes }
                case .posted(let text) { lines.append(text) }
                case .count(let n) { lines.append("count {n}") }
                case .user(let name) { lines.append("user {name}") }
            }
        }
    }

    /// Posts `text` to the room.
    func post(text: String) async throws CommandError {
        guard let channel else { throw CommandError.rpcFailure(.offline) }

        try await channel.send(.post(text: text))
    }
}
