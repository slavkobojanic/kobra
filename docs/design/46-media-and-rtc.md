# Media Capture, Recording and Real-Time Calls

## Decision

Media is two stdlib modules, both **client-only**, both reached through injected platform interfaces with a hermetic double in `k test`:

- **`k.media`**: capturing the camera, the microphone and the screen (`MediaDevices`), recording a stream to a `File` (`Recorder`), audio levels and sounds (`Audio`), and the `video` and `audio` elements, which play a stream, a `File` or a URL.
- **`k.rtc`**: peer-to-peer calls (`PeerConnections`) and **typed data channels**, with **signalling over a typed channel** of the app's own protocol ([44-channels.md](44-channels.md)) and a stdlib relay for the server side. The stdlib runs the whole offer/answer exchange, with "perfect negotiation", so an app never sees an SDP string.

Three rules hold everywhere:

- **Permission is a value, only `requestAccess` prompts, and denial is an error,** under the rules of [45-web-platform.md](45-web-platform.md#permissions). Whether the camera may be used is a reactive `PermissionState`; the prompt comes only from `requestAccess(for:)`, which the compiler allows only from a user's action; and a capture without a grant throws `CaptureError.notAsked` or `.denied`, which the compiler makes the app handle.
- **Every stream, recording, meter and peer has an owner** and is stopped with it ([03-reactivity.md](03-reactivity.md#async-work-is-owned)). Leaving the page turns the camera light off; no code has to remember to.
- **No third-party server is ever contacted.** There is no default STUN server. Two tabs on `localhost` connect with no servers at all; a deployment that needs relaying runs its own coturn next to the app, which hands browsers short-lived TURN credentials signed with a server-only secret.

```k
import k.media.MediaDevices
import k.media.MediaStream
import k.media.Recorder
import k.media.Recording
import k.media.RecordingLimit
import k.media.RecordingSession
import k.device.PermissionState

@Scope(.route)
public service MemoRecorder {
    @Inject let devices: MediaDevices
    @Inject let recorder: Recorder
    @Inject let memos: MemoService

    /// The page shows a Record button, or how to unblock the microphone.
    @Derived let microphone: PermissionState = devices.microphonePermission

    @State var session: RecordingSession? = nil
    @State var problem: MemoProblem? = nil

    // Called from the Record button's click handler, so it may prompt.
    func start() async {
        if devices.microphonePermission == .prompt {
            await devices.requestAccess(for: .microphone)      // @UserAction: the only prompt
        }

        do {
            let stream = try await devices.capture(.microphone, lifetime: .service)
            session = try recorder.start(stream, format: .voice, limit: RecordingLimit(duration: 4.minutes))
        } catch .notAsked, .denied {
            problem = .microphoneBlocked
        } catch .notFound {
            problem = .noMicrophone
        } catch .busy {
            problem = .microphoneInUse
        } catch let other {
            problem = .cannotRecord(other)
        }
    }

    func stop() async {
        guard let current = session else { return }
        session = nil

        do {
            let memo: Recording = try await current.stop()    // stops the microphone too
            await memos.upload(memo)
        } catch let failure {
            problem = .cannotRecord(failure)
        }
    }
}
```

## Capture: `MediaDevices`

```k
@Platform(side: .client)
public interface MediaDevices {
    var isSupported: Bool                          // false outside a secure context
    var cameraPermission: PermissionState          // reactive, as in 45-web-platform.md
    var microphonePermission: PermissionState
    var devices: List<MediaDevice>                 // reactive: plugged in, unplugged, labelled once granted

    @UserAction
    func requestAccess(for request: CaptureRequest) async -> PermissionState

    func capture(_ request: CaptureRequest, lifetime: CaptureLifetime = .owner) async throws CaptureError -> MediaStream

    @UserAction
    func captureScreen(_ request: ScreenCapture = ScreenCapture(), lifetime: CaptureLifetime = .owner) async throws CaptureError -> MediaStream
}
```

- **`requestAccess(for:)` is the only member that prompts for the camera or microphone,** following [45-web-platform.md](45-web-platform.md#permissions). It asks for every kind in the request in one prompt and returns `.granted` when all were granted, `.denied` when any was refused, and `.prompt` when the user dismissed it. Browsers have no prompt-only call, so it opens the devices (`getUserMedia`); the runtime keeps that stream for 5 seconds, and a `capture` of the same request in that time takes it over instead of opening the device again, so the camera light doesn't blink.
- **`capture` never prompts.** While the kind's permission is `.prompt` it throws `.notAsked`, while `.denied` it throws `.denied`. So a call page that already holds the grant captures in an effect as it opens, and the first ask always comes from a button ("Join call", "Record").
- **Some engines can't report the permission,** or keep a grant only for one visit (Firefox's default), and report `.prompt` after a reload. The app then shows its button again, and `requestAccess` answers at once, without a visible prompt, where the grant still holds. One more click on some browsers is the price of never prompting from an effect.
- **`captureScreen` shows the browser's screen picker** (`getDisplayMedia`). No engine keeps a grant for the screen, so it is both the request and the capture: it is `@UserAction`, like `Files.open` ([42-files.md](42-files.md#files)), and must be called from an event handler before that handler's first `await` (`user-action-required`, [45-web-platform.md](45-web-platform.md#user-activation)).
- **`devices`** lists the microphones, cameras and speakers, refreshed when one is plugged in or removed and after a grant. Browsers hide labels before a grant, so `MediaDevice.label` is `nil` until then. It is empty during server rendering and until the first enumeration, shortly after hydration.
- **Client-only.** `MediaDevices` is a platform interface, so only services inject it, and a service that does is client-only ([10-services.md](10-services.md#serverclient-boundary)). The stdlib ships one production implementation, `BrowserMediaDevices`, so `@Inject` needs no `use` rule.

### Requests

A request says what is captured, so a request for nothing can't be written:

```k
public enum CaptureRequest {
    case audio(AudioCapture)
    case video(VideoCapture)
    case audioAndVideo(AudioCapture, VideoCapture)

    static let microphone: CaptureRequest           // .audio(AudioCapture())
    static let camera: CaptureRequest               // .video(VideoCapture())
    static let cameraAndMicrophone: CaptureRequest  // .audioAndVideo(AudioCapture(), VideoCapture())
}

public struct AudioCapture {
    let device: MediaDeviceId? = nil
    let echoCancellation: Bool = true
    let noiseSuppression: Bool = true
    let autoGainControl: Bool = true
}

public struct VideoCapture {
    let device: MediaDeviceId? = nil
    let facing: CameraFacing? = nil       // .user, .environment: phones' front and back cameras
    let size: VideoSize? = nil            // an ideal, not a requirement: VideoSize(width: 1280, height: 720)
    let frameRate: Int? = nil             // an ideal
}

public struct ScreenCapture {
    let audio: Bool = false               // the tab's or system's sound, where the browser offers it
    let surface: DisplaySurface? = nil    // .tab, .window, .monitor: which the picker offers first
}
```

- **A chosen `device` is exact:** when it is gone, the capture throws `.notFound`, never silently opens another. `nil` means the browser's default.
- **Sizes and frame rates are ideals.** The browser picks the nearest it can, and the track's `settings` say what it chose. Hard constraints (`min`, `exact`) aren't offered: they are the usual source of `OverconstrainedError` on phones.
- `MediaDeviceId` is `Codable`, so a preferred microphone can be kept in `LocalStorage` ([36-local-storage.md](36-local-storage.md)). Browsers keep ids stable per origin until the user clears site data.

### Permission

`PermissionState` (`.prompt`, `.granted`, `.denied`) is `k.device`'s, defined in [45-web-platform.md](45-web-platform.md#permissions) and shared with geolocation and notifications.

- **`.denied` means asking is pointless.** A page shows how to change the site setting instead of a Record button that would fail. The memo example's view switches on `MemoRecorder.microphone`.
- `CaptureKind` (`.camera`, `.microphone`) names one kind, for the test double's `setPermission(_:to:)`.

### Errors

```k
public enum CaptureError implements Error, Equatable {
    case unsupported                    // no capture here: an insecure origin, a browser without it, a permissions policy
    case notAsked                       // the permission is .prompt: call requestAccess(for:) from a user's action first
    case denied                         // the user or the system refused
    case cancelled                      // the screen picker was closed without a choice
    case notFound                       // no device of the kind, or the chosen one is gone
    case busy                           // another application holds the device
    case unsatisfiable(setting: String) // the device can't do what was asked
}
```

| Browser error | `CaptureError` |
|---|---|
| `NotAllowedError` from `getUserMedia` | `.denied` |
| `NotAllowedError` from `getDisplayMedia` | `.cancelled`, or `.unsupported` when a permissions policy blocks it |
| `NotFoundError` | `.notFound` |
| `NotReadableError`, `AbortError` | `.busy` |
| `OverconstrainedError` | `.unsatisfiable(setting:)`, naming the constraint |
| no `navigator.mediaDevices` (not a secure context), `TypeError`, `SecurityError` | `.unsupported` |
| the permission is `.prompt` (checked before calling the browser) | `.notAsked` |

- **Capture works only in a secure context:** `https`, or `http://localhost` and `127.0.0.1`, which `k dev` serves. On any other `http` origin every capture is `.unsupported` and `isSupported` is `false`, and `k dev` prints why when it is bound to a LAN address.

### Lifetimes

```k
public enum CaptureLifetime {
    case owner      // stopped when the run that captured it is cancelled: its component unmounts, its page is left
    case service    // stopped when the injecting service's instance ends: the route, for @Scope(.route)
}
```

- **`.owner` is the default.** The capture call carries the owner of the run, as an rpc carries its `AbortSignal` ([03-reactivity.md](03-reactivity.md#async-work-is-owned)), and the owner's cancellation stops every track.
- **`.service` lets a service hold a stream across components,** such as a call that keeps running while the user opens a chat panel. A `@Scope(.app)` service's stream lives until it calls `stop()`; that is the only case where an app has to stop a stream itself.
- **A stream that arrives after its owner is gone is stopped at once.** If the user answers the prompt after leaving the page, the camera turns on for no frame: the runtime stops it before any K code sees it.
- `stop()` on a stream or a track may always be called early, and twice.

### Streams and tracks

`MediaStream` and `MediaTrack` are handles, not values: they don't implement `Codable`, so they can't be stored, sent in an rpc or put in a `@Load` field (each is a compile error, as for `Secret`). They implement `Equatable` by identity. **Their fields are reactive**, like `@State`: a view that reads `track.isMuted` updates when it changes.

| `MediaStream` | Meaning |
|---|---|
| `audioTracks`, `videoTracks: List<MediaTrack>` | the tracks |
| `isLive: Bool` | some track is still live |
| `isLocal: Bool` | captured on this device, rather than received from a peer |
| `stop()` | stops every track |

| `MediaTrack` | Meaning |
|---|---|
| `kind: TrackKind` | `.audio` or `.video` |
| `label: String` | what the browser calls the source ("MacBook Pro Microphone") |
| `device: MediaDeviceId?` | the device it came from; `nil` for a screen or a remote track |
| `isEnabled: Bool`, `setEnabled(_:)` | the app's mute: a disabled track sends silence or black frames, and keeps the device open |
| `isMuted: Bool` | the source's mute: the system or the peer isn't delivering, through no action of the app |
| `state: TrackState` | `.live` or `.ended(TrackEnd)` |
| `settings: TrackSettings` | the size, frame rate and audio settings the browser chose |
| `stop()` | ends the track and, once no track uses it, releases the device |

```k
public enum TrackEnd { case stopped, deviceLost, endedByUser, remoteEnded }
```

- `.endedByUser` is the browser's own "Stop sharing" button for a screen, or a permission revoked while capturing; `.deviceLost` a device unplugged.
- **Changing devices is a new capture,** not a mutation: capture the new microphone, give it to the peer with `replace` (below) or to the view, and stop the old one. Every step is ordinary and testable.

## Showing and playing media: `video` and `audio`

`video` and `audio` keep their HTML names ([05-views.md](05-views.md#built-in-element-names)) and take one required argument, what they play:

```k
public enum MediaSource {
    case stream(MediaStream)
    case file(File)
    case url(Url)
}
```

```k
view {
    if let camera = call.local {
        video(source: .stream(camera))
            .label("Your camera")
            .mirrored(true)
    }

    if let remote = call.remote {
        video(source: .stream(remote))
            .label("{call.peerName}'s camera")
    }

    for memo in memos.items key memo.id {
        audio(source: .file(memo.file))
            .label("Memo from {memo.recordedAt.formatted(.time)}")
    }
}
```

| Attribute | On | Meaning |
|---|---|---|
| `.label(String)` | both | the accessible name; tests find the element by it |
| `.controls(Bool)` | both | the browser's controls. Default `true` for a file or URL, `false` for a stream |
| `.muted(Bool)` | both | |
| `.monitor(Bool)` | both | play a **local** stream's sound. Default `false` (below) |
| `.mirrored(Bool)` | `video` | flips horizontally, for a self-view |
| `.poster(Url)` | `video` | |
| `.captions(Captions)` | `video` | `.track(url:language:label:)` or `.notNeeded` |
| `.loops(Bool)` | both | |
| `.controller(MediaController)` | both | drives playback from code (below) |
| events `ended`, `playbackBlocked` | both | |

- **Changing the source rebinds the element,** fine-grained like any other attribute: `srcObject` for a stream, a managed object URL for a file (revoked when the source changes or the element unmounts), `src` for a URL. Streams play inline (`playsinline`) and start by themselves.
- **A local stream never plays its own sound unless asked.** Hearing your own microphone through the speakers is feedback, so a local stream's audio is muted in both elements unless `.monitor(true)` says otherwise. A remote stream plays its sound.
- **Autoplay that the browser blocks is an event, not an error:** `playbackBlocked` fires when a stream or file with sound can't start without a user action, and the app shows a "Play" or "Unmute" button. A call starts from a click, so in practice it is rarely seen.
- **A `video` from a file or URL with no `.captions(…)` is a warning** (WCAG 1.2.2), silenced by `.captions(.notNeeded)` for a video without speech. Live streams are exempt: captions of a call are an [open question](#open-questions).
- During server rendering an element with a `.stream` renders as an empty element of the right size; streams only exist in the browser.

### `MediaController`

```k
component MemoPlayer {
    @Prop let memo: Memo

    let player = MediaController()

    view {
        audio(source: .file(memo.file))
            .controller(player)

        button { player.state == .playing ? "Pause" : "Play" }
            .on(click: { await player.toggle() })

        text { "{player.currentTime.formatted(.clock)} / {player.duration?.formatted(.clock) ?? "–"}" }
    }
}
```

| Member | Meaning |
|---|---|
| `play() async throws PlaybackError` | starts playing; `.blocked`, `.unsupportedFormat`, `.unreachable` |
| `pause()`, `toggle() async`, `seek(to: Duration)` | |
| `state: PlaybackState` | `.idle`, `.loading`, `.playing`, `.paused`, `.ended`, `.blocked`; reactive |
| `currentTime: Duration`, `duration: Duration?` | reactive; `duration` is `nil` until known and for live streams |
| `volume: Double`, `setVolume(_:)` | `0.0...1.0` |

A controller drives one element. Giving it to a second is a runtime error naming both, since which element it controls would otherwise depend on render order.

## Recording: `Recorder`

```k
@Platform(side: .client)
public interface Recorder {
    func start(_ stream: MediaStream, format: RecordingFormat, limit: RecordingLimit = .default) throws RecordingError -> RecordingSession

    func supports(_ format: RecordingFormat) -> Bool
}

public enum RecordingFormat implements Equatable {
    case voice      // audio for speech: Opus at 32 kbps in WebM, else AAC in MP4
    case audio      // audio for music: Opus at 128 kbps in WebM, else AAC in MP4
    case video      // VP9 or VP8 with Opus in WebM, else H.264 with AAC in MP4; 2.5 Mbps
    case exact(mediaType: String, audioBitsPerSecond: Int?, videoBitsPerSecond: Int?)
}
```

- **The named formats always work.** Each tries its WebM form, then its MP4 form (`MediaRecorder.isTypeSupported`), so Chromium and Firefox record WebM and Safari MP4; `recording.file.mediaType` says which. Only `.exact` can be `.unsupportedFormat`, and `supports(_:)` answers before trying.
- **`start` is synchronous** (so is `MediaRecorder.start`). The session belongs to the stream's owner: it is stopped, and its data discarded, with the stream.

| `RecordingSession` | Meaning |
|---|---|
| `state: RecordingState` | `.recording`, `.paused`, `.stopped(RecordingStop)`; reactive |
| `elapsed: Duration` | recorded time, paused time excluded, from the `Clock`; reactive, updated each second |
| `size: Int` | bytes so far; reactive |
| `pause()`, `resume()` | |
| `stop(keepStream: Bool = false) async throws RecordingError -> Recording` | finishes the file and, unless `keepStream` is `true`, stops the stream |

```k
public struct Recording implements Equatable {
    let file: File              // "recording.webm" or "recording.mp4", with its media type
    let duration: Duration
    let format: RecordingFormat
}

public enum RecordingStop { case byApp, streamEnded, limitReached }

public struct RecordingLimit {
    let size: Int = File.maximumSize       // 16 MiB
    let duration: Duration? = nil

    static let default: RecordingLimit
}

public enum RecordingError implements Error, Equatable {
    case unsupportedFormat      // only for .exact
    case noTrack                // e.g. .voice on a stream with no audio track
    case failed                 // the browser's encoder failed
}
```

- **A recording stops by itself** when its stream ends (the microphone was unplugged, the user stopped sharing) or it reaches its limit. Its `state` says why, and `stop()` still returns everything recorded until then. Nothing is lost silently.
- **A `Recording` is a `File`** ([42-files.md](42-files.md#file-and-filetype)): it is saved with `files.save(recording.file)`, played with `audio(source: .file(recording.file))`, and uploaded like any other file. The limit defaults to `File.maximumSize`, so every recording fits in a `File`.
- **Uploading** uses whatever [42-files.md](42-files.md) specifies for files. Until then an rpc with a `Bytes` field carries it, within the 1 MB request limit ([28-rpc-wire-format.md](28-rpc-wire-format.md)): at the `.voice` bitrate that is about four minutes of speech, so the memo example sets `RecordingLimit(duration: 4.minutes)`. Video doesn't fit, and needs the streaming upload in the open questions.
- `FileType` gains `.webm`, `.mp4`, `.m4a`, `.ogg` and `.mp3` (proposed here, added to 42-files.md's table when implemented), so a picker can offer recordings back to the app.

## Audio: levels and sounds

```k
@Platform(side: .client)
public interface Audio {
    func meter(for stream: MediaStream, lifetime: CaptureLifetime = .owner) throws AudioError -> AudioMeter

    func play(_ source: MediaSource) async throws PlaybackError
}
```

| `AudioMeter` | Meaning |
|---|---|
| `level: Double` | loudness now, `0.0...1.0` (RMS, on a perceptual scale); reactive, updated at most 30 times a second |
| `peak: Double` | the highest `level` of the last 1.5 seconds; reactive |
| `isSpeaking: Bool` | `level` above the speech threshold for 150 ms; reactive, for a "talking" indicator |
| `waveform(points: Int) -> List<Double>` | a snapshot of the last 50 ms, for drawing as an SVG polyline |
| `stop()` | |

- **One `AudioContext` per app,** created by the stdlib, resumed on the first user action on the page (browsers start it suspended). A meter reads `0.0` until then; a meter on a capture started from a click is live at once.
- **`k.media` ships `LevelMeter(meter:label:)`,** a component drawing `level` and `peak` as a bar, rendered as `role="meter"` whose `aria-valuenow` changes at most once a second, so a screen reader isn't flooded. A waveform or spectrum component waits for Canvas 2D in `k.graphics` ([45's Tier 2](45-web-platform.md#inventory)).
- **`play(_:)`** plays a short sound (a notification ding, a recorded memo) without an element, through the shared context. It resolves when the sound ends, and is cancelled with its owner like any `await`.
- Mixing, effects and synthesis (the rest of Web Audio) are [open questions](#open-questions); levels and short sounds are what applications ask for.

## Peer connections: `k.rtc`

### Signalling over a channel

Two browsers must exchange descriptions and network candidates before they can talk directly. **The app's protocol declares an ordinary channel rpc of `Signal`s** ([44-channels.md](44-channels.md)) and the handler hands both ends to the stdlib's `SignallingRelay`, so signalling is typed, guarded, authenticated, recorded in `rpc.lock` and reconnected like any other channel.

```k
import k.rtc.IceServer
import k.rtc.IceServers
import k.rtc.Signal
import k.rtc.SignallingRelay

public protocol CallProtocol requires Authenticated {
    rpc iceServers() throws RpcOnlyError -> List<IceServer> requires RateLimit(30.perMinute)

    rpc join(room: RoomId) throws CallError -> Channel<Signal, Signal, RpcOnlyError>
}

public enum CallError implements Error, Equatable {
    case notInvited
    case roomFull
    case rpcFailure(RpcFailure)
}

@Scope(.server)
public service CallRooms {
    let relay: SignallingRelay<RoomId> = SignallingRelay(peersPerRoom: 2)
}

public service CallApi implements CallProtocol {
    @Inject let rooms: CallRooms
    @Inject let ice: IceServers
    @Inject let invitations: InvitationRepository
    @Inject let session: Session

    rpc iceServers() async throws RpcOnlyError -> List<IceServer> {
        return await ice.forClient(user: session.user.id.text, validFor: 1.hour)
    }

    rpc join(room: RoomId) async throws CallError -> Channel<Signal, Signal, RpcOnlyError> {
        guard await invitations.allows(user: session.user.id, room: room) else {
            throw CallError.notInvited
        }
        guard let peer = rooms.relay.join(key: room) else {      // nil when the room is full
            throw CallError.roomFull
        }

        return Channel(sending: peer.signals) { signal => peer.forward(signal) }
    }
}
```

- **`Signal` is opaque.** It is a `Codable` stdlib enum (descriptions, candidates, the peer's role, "peer joined", "peer left") that only `PeerConnection` and `SignallingRelay` create or read. An app can pass it through its own code, but never builds an SDP string or parses one.
- **`SignallingRelay<Key>`** pairs the peers that join under one key, forwards each one's signals to the other, tells them their roles, and says `peerLeft` when one goes. `join(key:)` returns `nil` beyond `peersPerRoom`, so "room full" is the app's own typed open error. It is built on `Broadcast` and `Presence` ([44-channels.md](44-channels.md#presence-a-worked-example)), so the two peers can be on different replicas, and the `RelayPeer` it returns is a lease of the calling request: the peer leaves when its channel closes, and a crashed replica's peers leave after 30 seconds. `Key` is `Hashable` and `Codable`, as for `Broadcast`.
- **Who may join is the app's decision,** made in the handler before `join`, with the protocol's guards already run. The relay only ever forwards between peers of the same key.
- **The signalling channel reconnects as every channel does.** A resumed channel is invisible. A restarted one runs the handler again, rejoins the relay under the same role, and the impolite peer restarts ICE through it. Media and data keep flowing meanwhile, because they never pass through the server. When the channel closes for good, the peer connection closes with `.signallingLost`.
- The relay sees descriptions and candidates, which include the peers' network addresses. It never sees media or data-channel messages: those are encrypted end to end between the browsers (DTLS-SRTP, always on in WebRTC).

### `PeerConnections` and `PeerConnection`

```k
@Platform(side: .client)
public interface PeerConnections {
    var isSupported: Bool

    func connect(signalling: Channel<Signal, Signal, RpcOnlyError>, iceServers: List<IceServer>, policy: IcePolicy = .all, lifetime: CaptureLifetime = .owner) async throws PeerError -> PeerConnection
}
```

A full room is refused when the channel opens (`CallError.roomFull` above), so `connect` only waits until the relay pairs this peer with another and returns a `PeerConnection` in `.connecting`; media and data flow once it is `.connected`.

| `PeerConnection` | Meaning |
|---|---|
| `state: PeerState` | `.waitingForPeer`, `.connecting`, `.connected`, `.reconnecting`, `.closed(PeerClose)`; reactive |
| `send(_ stream: MediaStream) -> MediaSender` | sends every track of `stream` |
| `remote: MediaStream?` | what the peer sends; reactive, `nil` until its first track arrives |
| `channel<T>(_ kind: DataChannelKind<T>, delivery: Delivery = .reliable) -> DataChannel<T>` | a typed data channel (below) |
| `stats() async -> PeerStats` | round-trip time, bitrate, packet loss, and whether a relay is in use |
| `close()` | hangs up; the peer sees `.closed(.byPeer)` |

| `MediaSender` | Meaning |
|---|---|
| `replace(with: MediaStream)` | swaps the tracks being sent, with no renegotiation where the browser allows it: switching cameras, starting a screen share |
| `stop()` | stops sending these tracks |

```k
public enum PeerClose { case byApp, byPeer, failed, signallingLost }

public enum PeerError implements Error, Equatable {
    case unsupported            // no RTCPeerConnection in this browser
    case signallingClosed       // the channel ended before a peer joined
}
```

- **Negotiation is the stdlib's.** It implements the "perfect negotiation" pattern from the WebRTC specification: the relay names one peer *polite* and one *impolite*, offers that cross (glare) resolve by role, and every change that needs renegotiation (adding a track, opening a data channel, an ICE restart) runs through one queue. This is the part apps most often get wrong, and in K it isn't app code.
- **Reconnecting is automatic.** When the network changes (`iceConnectionState` `disconnected`), the state is `.reconnecting` and the impolite peer restarts ICE through the same channel; after 15 seconds without recovery it is `.closed(.failed)`. A UI shows "Reconnecting…" from `state` and needs no other code.
- **Owned like a capture:** `lifetime: .owner` closes the connection, and with it the channel, when the owner goes away. A route-scoped call service passes `.service`.
- `IcePolicy.relayOnly` forces traffic through TURN. It exists for testing a deployment's TURN server and for users who must not reveal their address to the peer.

### Typed data channels

```k
import k.rtc.DataChannelKind

public struct ChatLine implements Codable, Equatable {
    let text: String
    let sentAt: Date
}

let chatLines: DataChannelKind<ChatLine> = DataChannelKind(name: "chat")
let cursors: DataChannelKind<CursorPosition> = DataChannelKind(name: "cursor")

// In the call service:
let chat = peer.channel(chatLines)
try await chat.send(ChatLine(text: draft, sentAt: clock.now))

for await line in chat.messages {
    lines.append(line)
}

// Positions where only the latest matters:
let pointer = peer.channel(cursors, delivery: .unordered(retransmits: 0))
```

- **A kind pairs a name with a `Codable` message type,** like `DragKind<T>` ([41-drag-and-drop.md](41-drag-and-drop.md#kinds-and-payloads)) and `StorageKey<T>`. `send` takes only a `T`, and `messages` is a `Stream<T>`, so neither side can send or receive the wrong type. `DataChannelKind<Bytes>` sends raw bytes.
- **Both peers open a channel independently,** in any order. The stdlib negotiates it with an id derived from the name (`negotiated: true`), so there is no "who creates it" race. Two names whose ids collide in one app are a compile error asking to rename one.
- Two `DataChannelKind`s or `TabChannel`s with the same name and different types share one compile error with [45's `TabChannel`](45-web-platform.md#messages-between-tabs): **K2408** (`conflicting-channel`), the next free code after `K2406`. The check is built and keyed on `k.rtc.DataChannelKind`; it reports once `k.rtc` exists.
- **Messages are encoded like rpc messages:** binary Protobuf in production, JSON in development ([28-rpc-wire-format.md](28-rpc-wire-format.md)). When a channel opens, each side sends the fingerprint of its type's `Codable` shape. If the peers run different builds whose shapes differ, the channel closes with `DataChannelError.incompatible` on both sides, rather than decoding garbage.
- **`send` is async and applies backpressure:** it waits while more than 1 MiB is buffered, so a loop sending a file can't exhaust memory. A message over 64 KiB encoded throws `.tooLarge`, the size every browser pair delivers.
- `Delivery` is `.reliable` (ordered, retransmitted; the default) or `.unordered(retransmits: Int)`.

```k
public enum DataChannelError implements Error, Equatable {
    case closed
    case tooLarge(limit: Int)
    case incompatible
}
```

### STUN and TURN without third-party accounts

```k
public struct IceServer implements Codable, Equatable {
    let urls: List<Url>              // stun:, turn: and turns: URLs
    let username: String?
    let credential: String?          // short-lived and meant for the browser, so not a Secret
}

@Scope(.server)
public service IceServers {
    func forClient(user: String, validFor: Duration) async -> List<IceServer>
}

public config TurnConfig {
    @Env("K_TURN_URLS") let urls: List<Url> = []           // e.g. turn:turn.example.org:3478?transport=udp
    @Env("K_TURN_SECRET") let secret: Secret<String>?      // coturn's static-auth-secret
}
```

- **There is no default STUN or TURN server.** A browser on the same machine or the same network reaches the other with its own host candidates, so `k dev`, two tabs on `localhost`, and two laptops on one Wi-Fi need nothing. `IceServers.forClient` returns `[]` when `TurnConfig.urls` is empty. Defaulting to a public STUN server would send every user's address to a third party the app never chose.
- **A deployment that must work across networks runs coturn,** which serves STUN and TURN on one port. `IceServers` issues **time-limited credentials** with coturn's TURN REST scheme: the username is `<expiry>:<user>` and the credential the base64 HMAC-SHA1 of the username under the shared secret. HMAC-SHA1 is used only here, inside the stdlib, because coturn requires it; `k.crypto` doesn't expose it.
- **The secret stays on the server.** It is a server-only config ([25-config-and-secrets.md](25-config-and-secrets.md)), read from `K_TURN_SECRET_FILE` in containers; the browser gets credentials that expire with `validFor`, so a leaked credential is worth an hour of relaying.
- `TurnConfig` validates at startup: `K_TURN_URLS` set without a secret is a configuration error (exit 78), and so is a URL whose scheme isn't `stun`, `turn` or `turns`.

A Compose file next to the app ([38-deployment.md](38-deployment.md)):

```yaml
services:
  app:
    build: .
    environment:
      K_PUBLIC_ORIGIN: https://call.example.org
      K_TURN_URLS: turn:call.example.org:3478?transport=udp,turn:call.example.org:3478?transport=tcp
      K_TURN_SECRET_FILE: /run/secrets/turn_secret
    secrets: [turn_secret]

  turn:
    image: coturn/coturn:4.6
    network_mode: host                  # relay ports are a range; host networking avoids publishing each one
    secrets: [turn_secret]
    entrypoint: ["sh", "-c"]
    command:
      - >
        exec turnserver -n --no-cli --fingerprint
        --listening-port=3478 --min-port=49160 --max-port=49200
        --realm=call.example.org --external-ip=$${EXTERNAL_IP}
        --use-auth-secret --static-auth-secret="$$(cat /run/secrets/turn_secret)"
        --no-multicast-peers --denied-peer-ip=10.0.0.0-10.255.255.255
        --denied-peer-ip=172.16.0.0-172.31.255.255 --denied-peer-ip=192.168.0.0-192.168.255.255
        --denied-peer-ip=127.0.0.0-127.255.255.255
    environment:
      EXTERNAL_IP: 203.0.113.10

secrets:
  turn_secret:
    file: ./secrets/turn_secret
```

- **The denied ranges matter:** without them a TURN server relays to the deployment's private network, which turns it into a proxy into the data centre. The example's `README` says so next to the file, and the deployment test (below) checks the flags are present.
- Ports to open: 3478 TCP and UDP, and the relay range (49160–49200 UDP, enough for about 20 concurrent relayed calls). TLS (`turns:` on 5349) needs certificates; most deployments start with UDP and TCP on 3478, which carries already-encrypted media.

## Tests

`k test` installs a double for each interface ([21-testing.md](21-testing.md#test-doubles-and-defaults)). No prompt appears, no device opens and nothing leaves the process. A test reaches each with `@Inject` by its concrete type.

| Interface | Default in `k test` | What it does |
|---|---|---|
| `MediaDevices` | `TestMediaDevices` | one test microphone, camera and speaker; every permission `.prompt`, and prompts are granted unless the test says otherwise; captures return synthetic streams |
| `Recorder` | `TestRecorder` | records a synthetic stream into a small, deterministic file whose duration follows the `TestClock` |
| `Audio` | `TestAudio` | meters report the levels the test scripts; `played` lists every `play` |
| `PeerConnections` | `TestPeerConnections` | the real negotiation and relay over the real channel, with an in-process media engine; `peer(joining:)` makes a test-driven `TestCallPeer` |

### `TestMediaDevices`

| Member | Meaning |
|---|---|
| `setPermission(_ kind: CaptureKind, to: PermissionState)` | the state now, as if changed in site settings; views reading it update |
| `answerPrompts(with: PermissionState)` | how the simulated user answers `requestAccess` (`.granted` by default) |
| `prompts: Int` | `requestAccess` calls that showed a prompt |
| `failCaptures(with: CaptureError)` | every following capture throws |
| `cancelScreenPicker()` | the next `captureScreen` throws `.cancelled` |
| `add(_ device: MediaDevice)`, `remove(_ id: MediaDeviceId)` | plugs and unplugs; `remove` ends tracks with `.deviceLost` |
| `microphone: TestSource`, `camera: TestSource` | the default devices' sources |
| `captures: List<CaptureRequest>` | every request made |
| `liveTracks: List<MediaTrack>` | tracks not yet stopped, to check that leaving a page released the camera |

A `TestSource` scripts what a device produces, on the `TestClock`: `speak(level: 0.6, for: 2.seconds)`, `silence(for:)`, `stopSharing()` (ends a screen track with `.endedByUser`). Video frames are a solid test pattern; tests check streams, states and labels, never pixels.

```k
test MemosPage {
    @Inject let devices: TestMediaDevices
    @Inject let clock: TestClock

    test "a blocked microphone says how to allow it" {
        devices.setPermission(.microphone, to: .denied)

        let screen = render(MemosPage())

        expect !screen.button(named: "Record").exists
        expect screen.alert().text == "Microphone access is blocked. Allow it in your browser's site settings to record."
    }

    test "a recorded memo appears in the list" {
        let screen = render(MemosPage())

        screen.button(named: "Record").click()
        await screen.settle()
        devices.microphone.speak(level: 0.5, for: 3.seconds)
        clock.advance(by: 3.seconds)
        screen.button(named: "Stop").click()
        await screen.settle()

        expect screen.item(containing: "0:03").exists
        expect devices.liveTracks.isEmpty
    }

    test "leaving the page while recording turns the microphone off" {
        let screen = render(MemosPage())

        screen.button(named: "Record").click()
        await screen.settle()
        screen.unmount()

        expect devices.liveTracks.isEmpty
    }
}
```

### Loopback peers

The other side of a call is a **`TestCallPeer`** the test drives. It signs in as a second browser ([44-channels.md's `TestPeer`](44-channels.md#testpeer-a-second-browser)) and joins through the app's own protocol, so the guards, the handler's invitation check and the `SignallingRelay` all run exactly as in production; only the media engine is in-process. (A test can also render the call page in the second browser, for a test of two full UIs.)

```k
test CallPage {
    @Inject let rtc: TestPeerConnections
    @Inject let peers: TestPeers
    @Inject let devices: TestMediaDevices
    @Inject let sessions: SessionManager

    test "chat lines reach the other peer" {
        await sessions.signIn(userId: AccountId.sample)
        let screen = render(CallPage(room: RoomId.sample))
        screen.button(named: "Join call").click()          // requestAccess, then capture
        await screen.settle()

        let sam = await peers.signIn(userId: AccountId.sam)
        let samsApi: CallProtocol = sam.client()
        let other = try await rtc.peer(joining: try await samsApi.join(room: RoomId.sample))
        other.send(devices.camera.newStream())
        await screen.settle()

        expect screen.video(labeled: "Sam's camera").isPlaying

        screen.textField(labeled: "Message").fill("hello")
        screen.textField(labeled: "Message").press(.enter)
        await screen.settle()

        expect other.received(chatLines).map { line => line.text } == ["hello"]
    }

    test "a lost network shows reconnecting, then recovers" {
        // …joined as above…
        rtc.simulate(.networkLost)
        expect screen.status().text == "Reconnecting…"

        rtc.simulate(.networkRestored)
        await screen.settle()
        expect !screen.status().exists
    }
}
```

| `TestPeerConnections` / `TestCallPeer` | Meaning |
|---|---|
| `rtc.peer(joining: Channel<Signal, Signal, RpcOnlyError>) async throws PeerError -> TestCallPeer` | a test-driven peer on that channel |
| `rtc.simulate(_ event: NetworkEvent)` | `.networkLost`, `.networkRestored`, `.iceFails`, `.glare` (both sides offer at once) |
| `peer.send(_ stream:)`, `peer.close()` | as on a `PeerConnection` |
| `peer.channel(_ kind:)` | the test peer's end of a data channel, to send to the app |
| `peer.received(_ kind:) -> List<T>` | every message the app sent on that channel |
| `peer.remote: MediaStream?` | what the app sends; its tracks are the app's own, so a test can check that muting disabled them |

- **`screen.settle()` treats a peer waiting for signals, media or messages as idle,** as it does an open stream ([29-server-streaming.md](29-server-streaming.md#tests)).
- The element queries gain `video(labeled:)` and `audio(labeled:)`, with `isPlaying`, `isMuted` and `source`, so a test checks what a view plays without pixels.
- `IceServers` runs for real in tests, with `TurnConfig`'s built-in test value (no URLs), so `iceServers()` answers `[]`; a test that sets `use TurnConfig(…) for TurnConfig` gets credentials it can check against the documented HMAC.

### Testing the stdlib against real browsers

The doubles are only as good as their agreement with browsers, so the stdlib's own suite (not apps') runs the production implementations in the planned real-browser suite ([docs/compiler/testing.md §8](../compiler/testing.md#8-end-to-end-tests): Chromium driven over the DevTools protocol, skipped where no browser is installed, with Firefox over WebDriver BiDi after it), with the browsers' fake devices (`--use-fake-device-for-media-stream --use-fake-ui-for-media-stream`; `media.navigator.streams.fake`):

- each `CaptureError` row of the table above, provoked for real where the browser can be made to (a denied permission, a missing device, an insecure origin);
- recording in every named format, decoding the result in a `video` element to check its duration;
- two pages on `localhost` calling each other through a K server with no ICE servers, and the same through a coturn container with `IcePolicy.relayOnly`, checking `stats()` reports the relay;
- perfect negotiation under forced glare, a mid-call `replace`, and an ICE restart after the network namespace drops packets.

## Server and client

| Declaration | Side |
|---|---|
| `MediaDevices`, `Recorder`, `Audio`, `PeerConnections`, `MediaStream`, `MediaController`, `AudioMeter`, `PeerConnection`, `DataChannel` | client-only |
| `Signal`, `IceServer`, `Recording`, `DataChannelKind`, `PermissionState`, `MediaDeviceId` | both (values) |
| `SignallingRelay`, `RelayPeer`, `IceServers`, `TurnConfig` | server-only |

A server receives a recording as a `File`'s bytes and can store or serve them; it can't decode, transcode or inspect media. That would need a native codec library in the runtime, and is out of scope.

## Example apps

Both run with `k dev` and nothing else: no account, no external server.

- **`examples/memos`, a voice memo recorder.** Record with a live `LevelMeter`, pause and resume, stop; the memo uploads to the server (SQLite, `Bytes` in an entity, within the 1 MB rpc limit) and appears in a list with an `audio` player, a download (`files.save`) and delete. It handles every permission state: a Record button for `.prompt`, an explanation for `.denied`, a microphone picker remembered in `LocalStorage`, and a notice when the microphone is unplugged mid-recording. Its tests are the ones above.
- **`examples/call`, a 1:1 video call.** Sign in, create a room, open its link in a second tab or browser on the same machine. Camera and microphone toggles (`setEnabled`), switching camera, screen sharing through `MediaSender.replace`, a text chat on a typed data channel, a "Reconnecting…" status, and the connection's stats in a details panel. Two tabs on `localhost` connect with no ICE servers. Its `compose.yaml` adds coturn as above for a deployment, and a deployment test checks that `dist/app config`, the Compose file and coturn's flags agree (secrets as files only, the private ranges denied), like shop's ([38-deployment.md](38-deployment.md#the-toolchain-image)).

## As built (media round)

The media half of this design is built tests first; `k.rtc`, `examples/call` and Firefox are the next round (**Planned** `k-rtc` in the book). What was decided while building it:

- **Declarations.** `k.media` holds every type above (`crates/k_sema/stdlib/k/media`), `k.test` the doubles. `MediaDevices`, `Recorder` and `Audio` are `@Platform(side: .client)` interfaces with one `Browser…` implementation each, bound with no `use` rule; `MediaStream`, `MediaTrack`, `RecordingSession`, `AudioMeter` and `MediaController` are `@ClientOnly @Native` classes. The values of the second row of "Server and client" are plain structs and enums usable on both sides. `MediaDevices`' `cameraPermission`, `microphonePermission` and `devices` join `isSupported` and `permission` as the members the server may read while it renders (`.prompt`, `.prompt`, empty), and calling any other is K4012.
- **New diagnostics.** **K3112** (`missing-captions`, warning) is the captions rule: at the `source:` argument of a `video` written `.file(…)` or `.url(…)` with no `.captions(…)`, with a quick fix inserting `.captions(.notNeeded)`. **K4033** (`client-only-on-server`, error) is the rest of the boundary: a client-only stdlib class or value (`MediaStream`, `RecordingSession`, …) named in a server-only declaration. A stream's not being `Codable` needs no new code: an rpc parameter, a `@Load` field (K3021) and an entity column (K6002) already refuse it.
- **`RecordingLimit.default` isn't a member:** `default` is a reserved word in K. The default is `RecordingLimit()`, which is `File.maximumSize` and no duration, written as the parameter's default.
- **`AudioError`** (`.unsupported`, `.noTrack`) is the error of `Audio.meter`, which the sketch above left undeclared. `TrackSettings` has `size`, `frameRate`, `sampleRate`, `channelCount` and the three audio switches, each optional. `MediaDeviceKind` (`.microphone`, `.camera`, `.speaker`) is `MediaDevice.kind`.
- **`.service` is the page.** The runtime can't see which service a capture runs for, so a `.service` stream stops when the page that captured it is left (its path changes, or the document is unloaded), which is the route for `@Scope(.route)`, the case the design names. An `@Scope(.app)` service keeping a stream across pages isn't supported yet. In `k test` a `.service` stream stops when a screen unmounts.
- **`Audio.play(_:)`** plays through an `audio` element of its own rather than through the shared `AudioContext`: decoding a file in the context would need the whole file first, and the element streams it. Meters do use the one shared context.
- **The `video` and `audio` elements** lower to `$k.mediaElement`, a runtime feature in a chunk of its own (`playback.[hash].js`), so a page that plays a URL doesn't load the capture code. The server renders the bare element; the browser sets the source, `controls` and `muted` after hydration. `.captions(.track(…))` adds one `<track kind="captions" default>`.
- **Chunks.** `k.media`'s browser bodies are a feature chunk (`media.[hash].js`, 6 KB budget; about 4.4 KB minified), deferred to the pages that reach `MediaDevices`, `Recorder` or `Audio` like `Geolocation`; the shipped full client runtime didn't grow. `new MediaController()` and `LevelMeter` come from the same chunk; on the server the controller is an idle stand-in and the meter an empty `role="meter"`.
- **Defaults through an interface.** A call through an injected interface leaves a default argument out in the generated code, so the runtime defaults `limit`, `lifetime` and `ScreenCapture()` itself.
- **`File(name:mediaType:bytes:)`** compiles now: a stdlib struct's `@Native` fields (`File.text`, `File.size`) are computed, so they are no parameters of its memberwise initializer (it asked for them before).
- **Not built this round:** `FileType`'s `.webm`, `.mp4`, `.m4a`, `.ogg` and `.mp3` (42-files.md's table is unchanged; recordings carry their media type and are saved with `Files.save`, which needs none), `Duration.formatted(.clock)` (the memos example formats `m:ss` itself; the formatter would go into `std.js`, in the full runtime, which has 2 B of headroom), the `video(labeled:)` and `audio(labeled:)` queries (with `TestCallPeer`, for the call page), and `TestSource.newStream()`.
- **Doubles.** `TestMediaDevices` also has `setSupported(_:)`; its devices are `test-microphone`, `test-camera` and `test-speaker`, labelled once a kind was granted, and revoking a permission ends that kind's tracks with `.endedByUser`. `TestRecorder` records 100 bytes a second while recording and has `started` and `failNext(with:)`. `TestAudio`'s meters sample the stream's `TestSource` 30 times a second of the `TestClock`.
- **`examples/memos`** follows "Example apps" with two changes: an entity column can't hold `Bytes` yet, so the server stores a memo's audio as Base64 text; and a memo is at most three minutes or 750 kB, not four minutes, because the JSON wire format sends `Bytes` as Base64, a third larger, and the request limit is 1 MB. Its tests are the three above (with `not` for `!`, which K doesn't have) and three more: a refused prompt, paused time left out, and an unplugged microphone, whose recording is stored and announced.
- **Real browsers** (`crates/k_cli/tests/chrome.rs`): `media_capture_errors_in_chrome` provokes `.notAsked`, `.denied` (`Browser.setPermission`) and `.unsupported` (an `http` origin mapped to loopback that isn't `localhost`), then `.notFound` through Chrome's `OverconstrainedError`; `media_records_each_named_format_in_chrome` records `.voice`, `.audio` and `.video` and decodes each in a `video` element to check its duration; `memos_records_a_memo_and_plays_it_back` walks the example. `.cancelled` and `.busy` can't be provoked headless, and are covered by `media.test.js`'s scripted errors. On macOS, Chrome asks the system for the camera and microphone even when they are fake, and a test process that was never granted them waits forever: the parts that need a live stream check that `getUserMedia` answers within 5 seconds and are skipped with a message otherwise (`K_REQUIRE_MEDIA=1` makes them fail). They were skipped in the environment this round was built in.

## Why

- **Injected interfaces with doubles** keep the camera, the microphone and the network where the rest of the platform's I/O is: client-only by the compiler's rules, and testable in `k test` without a browser, a device or a second machine.
- **Typed permission and errors** turn the most common media bug, a page that assumes the user said yes, into a missing `catch` the compiler reports.
- **Ownership** fixes the second most common one, a camera left on after navigating away, without any cleanup code in the app.
- **Signalling over the app's own channel** reuses its sessions, guards and replicas instead of a separate signalling server, and keeps SDP out of app code, so the negotiation that apps usually get wrong is written once.
- **Typed data channels** give peer messages the guarantees rpc messages have, including across builds.
- **Self-hosted TURN with short-lived credentials** keeps the "no third-party account" rule and keeps the long-lived secret on the server.

## Alternatives rejected

- **Exposing `RTCPeerConnection` and SDP as typed wrappers.** Faithful, but every app would have to implement perfect negotiation, ICE restarts and glare handling itself; that is the pain this design removes. Lower-level access can be added later without changing this API.
- **A default public STUN server** (`stun.l.google.com`). It is free and common, but sends users' addresses to a third party and makes local development depend on the internet.
- **Signalling over a dedicated WebSocket endpoint.** It would bypass the protocol's guards and `rpc.lock`, and need its own fan-out between replicas, which `Broadcast` already provides.
- **Capture as a component** (`Camera { … }`, in the style of `KeyboardShortcut`). A stream is used by a recorder, a peer and a view at once, and by services that outlive one component; a value owned by its run fits better. The view part is the `video` element.
- **Recordings as `Stream<Bytes>`.** Needed for long recordings, but every current use fits in a `File`, which can be played, saved and uploaded with no new concepts.
- **Hard constraints (`exact`, `min`) on capture.** They fail on more devices than they help, and the ideal plus `settings` covers the uses seen.

## Open questions

- **Streaming upload** of long recordings and video: chunks from the recorder sent over a channel ([44-channels.md](44-channels.md)) or a streaming upload in [42-files.md](42-files.md), with a resumable server side.
- **Calls with more than two people:** a mesh (each peer to each) works up to about four; beyond that needs a selective forwarding unit, which would have to be self-hostable too.
- **Captions and transcription** of live audio, and serving large media files with range requests from endpoint pages.
- **Mixing, effects and synthesis** (the rest of Web Audio), and background blur or other processing of camera frames (insertable streams).
- **Picture-in-picture** and the Media Session API (lock-screen controls for playback).
- Whether `PermissionState` should also say *why* a permission is denied (by the user, the system or a policy), where browsers reveal it.
- **Choosing the speaker** (`setSinkId`), which WebKit lacks; cut from the first version ([45's Tier 3](45-web-platform.md#inventory)).
- **Waveform and spectrum components,** once Canvas 2D is designed in `k.graphics`.
