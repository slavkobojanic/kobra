// A call: a protocol whose channel carries `Signal`s, a handler that hands both ends to a
// `SignallingRelay`, ICE servers from `IceServers`, and a client service that connects, sends
// a stream and talks on a typed data channel (46-media-and-rtc.md, "Peer connections").
import k.auth.Authenticated
import k.auth.AuthUserId
import k.auth.Session
import k.crypto.Uuid
import k.media.MediaDevices
import k.media.MediaStream
import k.rpc.Channel
import k.rpc.RpcFailure
import k.rpc.RpcOnlyError
import k.rtc.DataChannel
import k.rtc.DataChannelKind
import k.rtc.IceServer
import k.rtc.IceServers
import k.rtc.MediaSender
import k.rtc.PeerConnection
import k.rtc.PeerConnections
import k.rtc.PeerState
import k.rtc.PeerStats
import k.rtc.Signal
import k.rtc.SignallingRelay

struct AccountId implements AuthUserId {
    let value: Uuid
}

struct ChatLine implements Codable, Equatable {
    let text: String
}

let chatLines: DataChannelKind<ChatLine> = DataChannelKind(name: "chat")

enum CallError implements Error {
    case roomFull
    case rpcFailure(RpcFailure)
}

protocol CallProtocol requires Authenticated {
    rpc iceServers() throws RpcOnlyError -> List<IceServer>

    rpc join(room: Uuid) throws CallError -> Channel<Signal, Signal, RpcOnlyError>
}

@Scope(.server)
service CallRooms {
    let relay: SignallingRelay<Uuid> = SignallingRelay(peersPerRoom: 2)
}

service CallApi implements CallProtocol {
    @Inject let rooms: CallRooms
    @Inject let ice: IceServers
    @Inject let session: Session

    rpc iceServers() async throws RpcOnlyError -> List<IceServer> {
        return await ice.forClient(user: "{session.user.id.value}", validFor: 1.hours)
    }

    rpc join(room: Uuid) async throws CallError -> Channel<Signal, Signal, RpcOnlyError> {
        guard let peer = await rooms.relay.join(key: room) else {
            throw CallError.roomFull
        }

        return Channel(sending: peer.signals) { signal => peer.forward(signal) }
    }
}

service Call {
    @Inject let api: CallProtocol
    @Inject let peers: PeerConnections
    @Inject let devices: MediaDevices

    @State var connection: PeerConnection? = nil
    @State var lines: List<ChatLine> = []

    @Derived let state: PeerState = connection?.state ?? .waitingForPeer
    @Derived let remote: MediaStream? = connection?.remote

    func join(room: Uuid) async throws CallError {
        var servers: List<IceServer> = []
        do {
            servers = try await api.iceServers()
        } catch .rpcFailure {
            servers = []
        }
        let signalling = try await api.join(room: room)

        do {
            connection = try await peers.connect(signalling: signalling, iceServers: servers)
        } catch .unsupported, .signallingClosed {
            return
        }

        guard let peer = connection else { return }

        do {
            let camera = try await devices.capture(.camera)
            let sender: MediaSender = peer.send(camera)
            sender.replace(with: camera)
        } catch .notAsked, .denied, .cancelled, .notFound, .busy, .unsatisfiable, .unsupported {
            peer.close()
        }

        let chat: DataChannel<ChatLine> = peer.channel(chatLines)

        for await line in chat.messages {
            lines.append(line)
        }
    }

    func say(text: String) async {
        guard let peer = connection else { return }

        do {
            try await peer.channel(chatLines).send(ChatLine(text: text))
        } catch .closed, .tooLarge, .incompatible {
            lines.append(ChatLine(text: "not sent"))
        }
    }

    func relayed() async -> Bool {
        guard let peer = connection else { return false }

        let stats: PeerStats = await peer.stats()
        return stats.isRelayed
    }
}
