import accounts.AuthService
import k.crypto.Uuid
import k.media.AudioCapture
import k.media.MediaDevice
import k.media.MediaDevices
import k.media.MediaStream
import k.media.VideoCapture
import k.rpc.Channel
import k.rpc.RpcOnlyError
import k.rtc.IceServer
import k.rtc.MediaSender
import k.rtc.PeerConnection
import k.rtc.PeerConnections
import k.rtc.PeerState
import k.rtc.PeerStats
import k.rtc.Signal

/// One call, from this browser (`docs/design/46-media-and-rtc.md`).
///
/// It injects the client-only `PeerConnections` and `MediaDevices`, so it
/// is client-only. Its `.route` scope, and the `.service` lifetime of the
/// capture and the connection, keep the call going while the page
/// re-renders, and end it when the page is left. The stdlib runs the whole
/// negotiation; this service only says what to send and shows what
/// arrives.
@Scope(.route)
public service CallService {
    @Inject let api: CallProtocol
    @Inject let peers: PeerConnections
    @Inject let devices: MediaDevices
    @Inject let auth: AuthService

    /// The call, once the other person is there.
    @State var connection: PeerConnection? = nil

    /// This browser's camera and microphone.
    @State var camera: MediaStream? = nil

    /// The screen being shared instead of the camera.
    @State var screen: MediaStream? = nil

    /// What the connection sends: the camera, or the screen.
    @State var sender: MediaSender? = nil

    /// `true` from joining the room until hanging up.
    @State var joined: Bool = false

    /// The chat, oldest first.
    @State var lines: List<ChatEntry> = []

    /// The other person's name, once they introduced themselves.
    @State var peerName: String? = nil

    /// Why the call couldn't start, or a control failed.
    @State var problem: CallProblem? = nil

    /// The connection's latest statistics.
    @State var stats: PeerStats? = nil

    /// Whether this browser can make calls here.
    @Derived let canCall: Bool = peers.isSupported and devices.isSupported

    /// Whether the camera or the microphone is blocked in the site's
    /// settings, so a Join button would fail.
    @Derived let blocked: Bool = devices.cameraPermission == .denied or devices.microphonePermission == .denied

    /// What the other person sends.
    @Derived let remote: MediaStream? = connection?.remote

    /// Where the call is; `nil` before joining.
    @Derived let state: PeerState? = connection?.state

    /// What the status line says, or `nil` while the call is live.
    @Derived let banner: String? = callBanner(joined: joined, state: connection?.state)

    /// Whether the microphone sends sound (the app's mute, not the device's).
    @Derived let microphoneOn: Bool = camera?.audioTracks.all { t => t.isEnabled } ?? false

    /// Whether the camera sends pictures.
    @Derived let cameraOn: Bool = camera?.videoTracks.all { t => t.isEnabled } ?? false

    /// The cameras to switch between.
    @Derived let cameras: List<MediaDevice> = devices.devices.filter { d => d.kind == .camera }

    /// The name this browser introduces itself with.
    @Derived let myName: String = auth.user?.id.name ?? "Someone"

    // Called from the Join button's click handler, so it may prompt.
    /// Asks for the camera and the microphone the first time, joins the
    /// room, and waits for the other person; then talks until either hangs
    /// up.
    func join(room: Uuid) async {
        problem = nil
        if devices.cameraPermission == .prompt or devices.microphonePermission == .prompt {
            await devices.requestAccess(for: .cameraAndMicrophone)      // @UserAction: the only prompt
        }

        do {
            camera = try await devices.capture(.cameraAndMicrophone, lifetime: .service)
        } catch .notAsked, .denied {
            problem = .devicesBlocked
        } catch .notFound {
            problem = .noDevices
        } catch .busy {
            problem = .devicesInUse
        } catch let other {
            problem = .cannotCapture(other)
        }
        guard let local = camera else { return }

        var servers: List<IceServer>? = nil
        do {
            servers = try await api.iceServers()
        } catch .rpcFailure {
            problem = .offline
        }

        var signalling: Channel<Signal, Signal, RpcOnlyError>? = nil
        if servers != nil {
            do {
                signalling = try await api.join(room: room)
            } catch .roomFull {
                problem = .roomFull
            } catch .rpcFailure {
                problem = .offline
            }
        }
        guard let channel = signalling else {
            local.stop()
            camera = nil
            return
        }

        joined = true
        var connected: PeerConnection? = nil
        do {
            connected = try await peers.connect(signalling: channel, iceServers: servers ?? [], lifetime: .service)
        } catch .unsupported {
            problem = .unsupported
        } catch .signallingClosed {
            problem = .offline
        }
        guard let peer = connected else {
            hangUp()
            return
        }

        connection = peer
        sender = peer.send(local)
        await talk(on: peer)
    }

    /// Introduces this person, then follows the chat and the other
    /// person's introduction until the call ends.
    private func talk(on peer: PeerConnection) async {
        let chat = peer.channel(chatLines)
        let names = peer.channel(introductions)

        do {
            try await names.send(Introduction(name: myName))
        } catch .closed, .tooLarge, .incompatible {
            // Without a name the page says "the other person".
        }

        let events = Stream.merge(
            chat.messages.map { line => CallEvent.line(line) },
            names.messages.map { name => CallEvent.introduced(name) }
        )
        for await event in events {
            match event {
                case .line(let line) {
                    lines.append(ChatEntry(number: lines.count + 1, line: line))
                }

                case .introduced(let introduction) {
                    peerName = introduction.name
                }
            }
        }
    }

    /// Sends a chat line; `false` when it couldn't be sent.
    func say(text: String) async -> Bool {
        let trimmed = text.trimmed()
        guard let peer = connection, trimmed.isNotEmpty else { return false }

        let line = ChatLine(from: myName, text: trimmed)
        do {
            try await peer.channel(chatLines).send(line)
        } catch .closed, .tooLarge, .incompatible {
            return false
        }
        lines.append(ChatEntry(number: lines.count + 1, line: line))
        return true
    }

    /// Mutes or unmutes the microphone. The track stays open, so unmuting
    /// is instant.
    func setMicrophone(on: Bool) {
        for track in camera?.audioTracks ?? [] {
            track.setEnabled(on)
        }
    }

    /// Turns the camera's picture off or on.
    func setCamera(on: Bool) {
        for track in camera?.videoTracks ?? [] {
            track.setEnabled(on)
        }
    }

    /// Switches to the next camera: a new capture, given to the peer with
    /// `replace`, so the call doesn't renegotiate; then the old one stops.
    func switchCamera() async {
        guard let current = camera, cameras.count > 1 else { return }

        let using = current.videoTracks.first?.device
        let at = cameras.indexWhere { d => d.id == using } ?? -1
        let next = cameras[(at + 1) % cameras.count]
        let wasMuted = not microphoneOn

        do {
            let switched = try await devices.capture(.audioAndVideo(AudioCapture(), VideoCapture(device: next.id)), lifetime: .service)
            if screen == nil {
                sender?.replace(with: switched)
            }
            current.stop()
            camera = switched
            setMicrophone(on: not wasMuted)
        } catch let failure {
            problem = .cannotCapture(failure)
        }
    }

    // Called from the Share button's click handler: the screen picker is a
    // user's action.
    /// Shares a screen, a window or a tab instead of the camera; the
    /// microphone keeps sending.
    func shareScreen() async {
        do {
            let shared = try await devices.captureScreen(lifetime: .service)
            screen = shared
            sender?.replace(with: shared)
        } catch .cancelled {
            // The picker was closed: keep the camera.
        } catch .notAsked, .denied, .notFound, .busy, .unsatisfiable, .unsupported {
            problem = .cannotShare
        }
    }

    /// Goes back to the camera, also when the browser's own "Stop sharing"
    /// button ended the screen.
    func stopSharing() {
        guard let shared = screen else { return }

        if let local = camera {
            sender?.replace(with: local)
        }
        shared.stop()
        screen = nil
    }

    /// Reads the connection's round-trip time, bitrate and relay.
    func refreshStats() async {
        guard let peer = connection else {
            stats = nil
            return
        }

        stats = await peer.stats()
    }

    /// Hangs up: the other person sees the call end, and the camera, the
    /// microphone and the screen turn off.
    func hangUp() {
        connection?.close()
        screen?.stop()
        camera?.stop()
        screen = nil
        camera = nil
        sender = nil
        joined = false
    }
}

/// The status line for a call: `nil` before joining and while live.
func callBanner(joined: Bool, state: PeerState?) -> String? {
    guard joined else { return nil }

    guard let current = state else {
        return "Waiting for someone to join. Send them this page's link."
    }

    return match current {
        case .waitingForPeer { "The other person left. Waiting for someone to join." }
        case .connecting { "Connecting…" }
        case .connected { nil }
        case .reconnecting { "Reconnecting…" }
        case .closed(.byPeer) { "The other person hung up." }
        case .closed(.failed) { "The connection was lost." }
        case .closed(.signallingLost) { "Lost contact with the server." }
        case .closed(.byApp) { nil }
    }
}
