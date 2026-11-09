import k.media.MediaDevices
import k.media.MediaStream
import k.media.VideoCapture
import k.media.VideoSize
import k.rtc.IceServer
import k.rtc.MediaSender
import k.rtc.PeerConnection
import k.rtc.PeerConnections
import k.rtc.PeerState

/// Joins, sends, replaces and measures one way per button, and says what
/// happened.
@Scope(.route)
public service CallLab {
    @Inject let api: LabProtocol
    @Inject let peers: PeerConnections
    @Inject let devices: MediaDevices

    /// What the last button did.
    @State var status: String = "ready"

    /// The call.
    @State var connection: PeerConnection? = nil

    /// What it sends.
    @State var sender: MediaSender? = nil

    /// The connection's state, as a word.
    @Derived let state: String = stateName(connection?.state)

    /// What the other tab sends.
    @Derived let remote: MediaStream? = connection?.remote

    /// Joins room `lab`; through TURN only when `relayed`.
    func join(relayed: Bool) async {
        var servers: List<IceServer> = []
        if relayed {
            do {
                servers = try await api.iceServers()
            } catch .rpcFailure {
                status = "no ICE servers"
                return
            }
        }
        status = "joining with {servers.count} ICE servers"

        do {
            let signalling = try await api.join(room: "lab")
            connection = try await peers.connect(signalling: signalling, iceServers: servers, policy: if relayed { .relayOnly } else { .all }, lifetime: .service)
            status = "paired"
        } catch .full {
            status = "join failed: full"
        } catch .rpcFailure {
            status = "join failed: offline"
        } catch .unsupported {
            status = "join failed: unsupported"
        } catch .signallingClosed {
            status = "join failed: signallingClosed"
        }
    }

    /// Sends the camera at `width` by three quarters of it, or replaces
    /// what is sent with it.
    func send(width: Int) async {
        guard let peer = connection else {
            status = "not joined"
            return
        }

        await devices.requestAccess(for: .camera)
        do {
            let camera = try await devices.capture(.video(VideoCapture(size: VideoSize(width: width, height: width * 3 / 4))), lifetime: .service)
            if let current = sender {
                current.replace(with: camera)
                status = "replaced with {width}"
            } else {
                sender = peer.send(camera)
                status = "sending {width}"
            }
        } catch _ {
            status = "capture failed"
        }
    }

    /// Reads the connection's statistics.
    func measure() async {
        guard let peer = connection else {
            status = "not joined"
            return
        }

        let stats = await peer.stats()
        status = "stats relayed={stats.isRelayed}"
    }

    /// Hangs up.
    func hangUp() {
        connection?.close()
        status = "hung up"
    }
}

/// A state's case, as a word.
func stateName(_ state: PeerState?) -> String {
    guard let current = state else { return "none" }

    return match current {
        case .waitingForPeer { "waitingForPeer" }
        case .connecting { "connecting" }
        case .connected { "connected" }
        case .reconnecting { "reconnecting" }
        case .closed(.byApp) { "closed byApp" }
        case .closed(.byPeer) { "closed byPeer" }
        case .closed(.failed) { "closed failed" }
        case .closed(.signallingLost) { "closed signallingLost" }
    }
}
