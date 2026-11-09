import k.rpc.Channel
import k.rpc.ChannelState
import k.rpc.RpcOnlyError

/// Follows the lobby from the browser.
public service LobbyService {
    @Inject let api: LobbyProtocol

    @State var lines: List<String> = []
    @State var channel: Channel<LobbyCommand, LobbyEvent, CommandError>? = nil

    @Derived let connection: ChannelState = channel?.state ?? .connecting

    /// What the banner says, or `nil` while the channel is open.
    @Derived let banner: String? = match connection {
        case .connecting { "Connecting…" }
        case .open { nil }
        case .reconnecting { "Reconnecting…" }
        case .closed { "Disconnected." }
    }

    /// Opens the lobby and follows it until the caller is cancelled.
    func follow() async throws RpcOnlyError {
        let opened = try await api.lobby()

        await listen(on: opened)
    }

    /// Sends on `opened` from now on.
    func attach(_ opened: Channel<LobbyCommand, LobbyEvent, CommandError>) {
        channel = opened
    }

    /// Follows an open channel until it closes.
    func listen(on opened: Channel<LobbyCommand, LobbyEvent, CommandError>) async {
        attach(opened)

        for await event in opened.messages {
            match event {
                case .snapshot(let all) { lines = all }
                case .said(let line) { lines.append(line) }
            }
        }
    }

    /// Says `text` in the lobby.
    func say(text: String) async throws CommandError {
        guard let channel else {
            throw CommandError.rpcFailure(.offline)
        }

        try await channel.send(.say(text: text))
    }
}
