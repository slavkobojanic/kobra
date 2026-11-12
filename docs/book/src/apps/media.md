# Camera, microphone, recording and calls

Capturing the camera, the microphone and the screen, recording what they produce, measuring how
loud it is and playing media back are the **`k.media`** module; calls between two browsers are
**`k.rtc`** ([below](#calls-between-browsers)). Its three interfaces,
`MediaDevices`, `Recorder` and `Audio`, are client-only and injected into services, like
`Geolocation`; `k test` replaces each with a double, so no test opens a prompt or a device. The
design is in `docs/design/46-media-and-rtc.md`.

Three rules hold everywhere:

- **Permission is a value, only `requestAccess(for:)` prompts, and denial is an error.**
- **Every stream, recording and meter has an owner** and stops with it: leaving the page turns the
  microphone off, with no cleanup code.
- **Nothing leaves the browser** unless the app sends it, as it sends any other value.

## Capturing

```k
// examples/memos/src/recording/MemoRecorder.k
@Scope(.route)
public service MemoRecorder {
    @Inject let devices: MediaDevices
    @Inject let recorder: Recorder
    @Inject let audio: Audio
// ...
    /// The page shows a Record button, or how to unblock the microphone.
    @Derived let microphone: PermissionState = devices.microphonePermission
// ...
    func start() async {
        problem = nil
        notice = nil
        if devices.microphonePermission == .prompt {
            await devices.requestAccess(for: .microphone) // @UserAction: the only prompt
        }

        var captured: MediaStream? = nil
        do {
            captured = try await devices.capture(.audio(AudioCapture(device: chosen)), lifetime: .service)
        } catch .notAsked, .denied {
            problem = .microphoneBlocked
        } catch .notFound {
            problem = .noMicrophone
        } catch .busy {
            problem = .microphoneInUse
        } catch let other {
            problem = .cannotCapture(other)
        }
        guard let stream = captured else { return }
// ...
    }
```

- **`requestAccess(for:)` is the only member that prompts.** It is `@UserAction`, so it is called
  from a click, submit or key handler before the handler's first `await`, or the compiler reports
  K4030. It asks for every kind in the request at once and returns the new `PermissionState`:
  `.granted`, `.denied`, or `.prompt` when the user dismissed the prompt.
- **`capture(_:lifetime:)` never prompts.** While the kind's permission is `.prompt` it throws
  `CaptureError.notAsked`, and while it is `.denied`, `.denied`. A page that already holds the grant
  can capture in an effect as it opens; the first ask always comes from a button.
- **A `CaptureRequest` says what is captured:** `.microphone`, `.camera`, `.cameraAndMicrophone`,
  or `.audio(AudioCapture(…))`, `.video(VideoCapture(…))` and `.audioAndVideo(…, …)` with a chosen
  `device`, echo cancellation and the other settings. A chosen device is exact: when it is gone the
  capture throws `.notFound`. Sizes and frame rates are ideals the browser comes as close to as it
  can.
- **`CaptureError` is handled by type**, so a page that assumes the user said yes is a missing
  `catch` the compiler reports: `.unsupported` (no capture here: an `http` origin other than
  `localhost`), `.notAsked`, `.denied`, `.cancelled` (the screen picker closed), `.notFound`,
  `.busy` (another application holds the device) and `.unsatisfiable(setting:)`.
- **`captureScreen(_:lifetime:)`** shows the browser's screen picker. No browser keeps a grant for
  the screen, so it is the request and the capture at once, and `@UserAction` too.
- **`cameraPermission`, `microphonePermission`, `isSupported` and `devices` are reactive.** While
  the server renders they are `.prompt`, `.prompt`, `false` and empty, so they may be read in a
  `@Derived` value a page shows; any other member is K4012 there. `devices` lists the microphones,
  cameras and speakers, with labels once the user has granted a capture; a `MediaDeviceId` is
  `Codable`, so a chosen microphone can be kept in `LocalStorage`.

### Streams, tracks and lifetimes

A `MediaStream` is a handle, not a value: it isn't `Codable`, so it can't be stored, sent in an rpc
or put in a `@Load` field, and naming one in server-only code is K4033. Its fields are reactive:
`audioTracks`, `videoTracks`, `isLive` and `isLocal`, and `stop()` stops every track. A
`MediaTrack` has its `kind`, `label`, `device`, `isEnabled` (the app's mute, `setEnabled(_:)`),
`isMuted` (the source's), `state` (`.live`, or `.ended` with a `TrackEnd`: `.stopped`,
`.deviceLost`, `.endedByUser`) and the `settings` the browser chose.

The `lifetime` says what a stream belongs to:

- **`.owner`, the default:** the run that captured it. A capture from a component's handler stops
  when the component unmounts.
- **`.service`:** the injecting service's instance, the page for a `@Scope(.route)` service, so a
  stream outlives the component that started it and still stops when the page is left.

A stream that arrives after its owner is gone, because the user answered the prompt after leaving
the page, is stopped before any K code sees it.

## Recording

```k
// examples/memos/src/recording/MemoRecorder.k
        do {
            // Three minutes of speech fit in one 1 MB rpc request.
            session = try recorder.start(stream, format: .voice, limit: RecordingLimit(size: 750000, duration: 3.minutes))
            startedAt = clock.now
        } catch let failure {
            stream.stop()
            meter = nil
            problem = .cannotRecord(failure)
        }
```

- **`start(_:format:limit:)` returns a `RecordingSession`** whose `state` (`.recording`, `.paused`,
  `.stopped` with a `RecordingStop`), `elapsed` and `size` are reactive. `pause()` and `resume()`
  leave the paused time out, and `stop(keepStream:)` finishes the file and, unless told to keep it,
  stops the stream.
- **The named formats always work.** `.voice` (speech, Opus at 32 kbps), `.audio` (music, 128 kbps)
  and `.video` try their WebM form, then their MP4 form, so Chromium and Firefox record WebM and
  Safari MP4. Only `.exact(mediaType:…)` can throw `RecordingError.unsupportedFormat`, and
  `supports(_:)` answers before trying. A stream without the track a format needs throws
  `.noTrack`.
- **A recording stops by itself** when its stream ends (the microphone was unplugged) or it reaches
  its `RecordingLimit`, which is `File.maximumSize` by default. Its `state` says why, and `stop()`
  still returns everything recorded until then.
- **A `Recording` is a `File`** with its `duration` and `format`: save it with `files.save`, play it
  with `audio(source: .file(…))`, or send its bytes to the server, which stores them but can't
  decode them.

## Levels and sounds

`Audio.meter(for:lifetime:)` measures a stream's loudness: an `AudioMeter` whose `level` and `peak`
(`0.0…1.0`) and `isSpeaking` are reactive. **`LevelMeter(meter:label:)`** draws it as a bar with
`role="meter"`, whose value a screen reader hears at most once a second:

```k
// examples/memos/src/recording/RecorderPanel.k
        } else if let session = recorder.session {
            if let meter = recorder.meter {
                LevelMeter(meter: meter, label: "Microphone level")
            }
```

`Audio.play(_:)` plays a short sound without an element and resolves when it ends.

## Playing media: `video` and `audio`

The `video` and `audio` elements take what they play as a `MediaSource`: `.stream(…)`, `.file(…)`
or `.url(…)`.

```k
// examples/memos/src/memos/MemoRow.k
        if let file {
            audio(source: .file(file))
                .label("Memo from {memo.recordedAt.formatted(as: .time)}")
        }
```

- `.label` is the accessible name, `.controls` shows the browser's controls (by default for a file
  or a URL, not for a stream), `.muted`, `.loops`, and for `video` `.mirrored` (a self-view),
  `.poster` and `.captions`.
- **A local stream plays no sound of its own** unless `.monitor(true)`: hearing your own
  microphone through the speakers is feedback.
- **A `video` of a file or URL needs captions** (WCAG 1.2.2): `.captions(.track(url:language:label:))`
  with a WebVTT file, or `.captions(.notNeeded)` for a video without speech. Without either the
  compiler warns (K3112), and its quick fix inserts `.captions(.notNeeded)`. A live stream is
  exempt.
- The events are `ended` and `playbackBlocked`, which fires when the browser won't start sound
  without a user's action.
- **`MediaController`** drives one element from code: `.controller(player)`, then `play()`,
  `pause()`, `toggle()`, `seek(to:)` and `setVolume(_:)`, with reactive `state`, `currentTime`,
  `duration` and `volume`. Giving one controller to two elements is a run-time error naming both.

## Testing

`k test` puts **`TestMediaDevices`**, **`TestRecorder`** and **`TestAudio`** in place of the
browser's. There is one test microphone, camera and speaker; both permissions start at `.prompt`;
prompts are answered `.granted`; captures return synthetic streams; recordings follow the
`TestClock`; and meters read the levels the test scripts:

```k
// examples/memos/src/pages/MemosPage.test.k
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
```

`setPermission(_:to:)` changes a permission as the site's settings would, `answerPrompts(with:)`
sets how the user answers, `failCaptures(with:)` fails every following capture,
`cancelScreenPicker()` closes the next picker, and `add(_:)` and `remove(_:)` plug devices in and
out (a removed device's tracks end with `.deviceLost`). `prompts`, `captures` and `liveTracks`
record what happened, so a test can check that leaving a page released the microphone.

## Calls between browsers

**`k.rtc`** connects two browsers directly: their cameras, microphones and screens, and **typed
data channels** for anything else. The two find each other through a **channel of the app's own
protocol** ([Channels](channels.md)), so signalling is typed, guarded and reconnected like any
other channel, and the stdlib runs the whole offer/answer exchange: an app never sees a session
description. Media and data go from browser to browser, encrypted end to end; the server only
pairs the two.

### Signalling over a channel

The protocol declares a channel rpc of `Signal`s, and its handler hands both ends to a
`SignallingRelay`, which pairs the people who join one room and forwards each one's signals to the
other:

```k
// examples/call/src/calls/CallProtocol.k
public protocol CallProtocol requires Authenticated {
    /// The STUN and TURN servers to connect through, with credentials that
    /// expire in an hour; none in development.
    rpc iceServers() throws RpcOnlyError -> List<IceServer> requires RateLimit(30.perMinute)
// ...
    /// Joins a room's signalling until the caller leaves it.
    rpc join(room: Uuid) throws CallError -> Channel<Signal, Signal, RpcOnlyError>
}
```

```k
// examples/call/src/calls/CallApi.k
    rpc join(room: Uuid) async throws CallError -> Channel<Signal, Signal, RpcOnlyError> {
        // In the room until this channel closes: leaving the page, a
        // dropped connection that doesn't come back, or signing out.
        guard let peer = await rooms.relay.join(key: room) else {
            throw CallError.roomFull
        }

        return Channel(sending: peer.signals) { signal => peer.forward(signal) }
    }
```

- **`SignallingRelay<Key>(peersPerRoom:)`** is held by a `@Scope(.server)` service. `join(key:)`
  answers `nil` when the room is full, so "room full" is the app's own typed error. The peer is a
  lease of the channel's request: it leaves when the channel closes, with nothing to clean up.
- The relay is built on `Presence` and `Broadcast`, so with Postgres the two peers may be on
  different replicas.
- **Who may join is the app's decision**, made by the guards and the handler before `join`. The
  call example's rooms are unguessable links; another app checks an invitation first.
- **`Signal` is opaque**: only the stdlib creates or reads one.

### Connecting

On the client, `PeerConnections` (a client-only platform interface, like `MediaDevices`) connects
over that channel. `connect` waits until the relay pairs this browser with another, then returns a
`PeerConnection`:

```k
// examples/call/src/calls/CallService.k
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
```

| `PeerConnection` | Meaning |
|---|---|
| `state` | `.waitingForPeer`, `.connecting`, `.connected`, `.reconnecting` or `.closed(PeerClose)`; reactive |
| `remote` | what the other browser sends, a `MediaStream`; `nil` until its first track arrives |
| `send(_:)` | sends a stream's tracks, and returns a `MediaSender` |
| `channel(_:delivery:)` | a typed data channel (below) |
| `stats()` | round-trip time, bitrate, packet loss, and whether a TURN relay is in use |
| `close()` | hangs up: the other side sees `.closed(.byPeer)` |

- **Negotiation is the stdlib's.** It implements the WebRTC specification's "perfect negotiation":
  the two browsers' ids make one polite and one impolite, offers that cross resolve by role, and
  sending a stream or opening a channel needs no app code.
- **Reconnecting is automatic.** When the network changes, the state is `.reconnecting` and the
  impolite side restarts ICE through the same channel; after 15 seconds without recovery the call
  closes `.failed`. A page shows "Reconnecting…" from `state` and nothing else.
- **A call is owned like a capture**: `lifetime: .owner` hangs up when the run that connected is
  cancelled, `.service` when the page is left.
- **`MediaSender.replace(with:)`** swaps what is sent without renegotiating: switching cameras,
  sharing the screen. Each of the new stream's tracks replaces the one of its kind, so sharing a
  screen keeps the microphone. `stop()` stops sending, and the other side's copies end with
  `.remoteEnded`.
- Muting is `MediaTrack.setEnabled(false)` on the local track: the other side receives silence or
  black frames, and the device stays open.

### Typed data channels

```k
// examples/call/src/calls/chatLines.k
/// The call's chat: typed lines, straight from one browser to the other.
public let chatLines: DataChannelKind<ChatLine> = DataChannelKind(name: "chat")
```

A `DataChannelKind<T>` pairs a name with a `Codable` message type, and `peer.channel(chatLines)`
gives a `DataChannel<ChatLine>` whose `send(_:)` takes only a `ChatLine` and whose `messages` is a
`Stream<ChatLine>`. Both sides open the channel independently, in any order: its id is derived from
the name. Two kinds with the same name and different types are K2408, and two names whose ids
collide are K2409. When a channel opens, each side checks that the other's message type has the
same shape, so two builds that disagree close it `.incompatible` instead of decoding the wrong
type. `send` waits while more than 1 MiB is buffered and refuses a message over 64 KiB
(`.tooLarge`). `delivery: .unordered(retransmits: 0)` suits values where only the latest matters,
such as a pointer.

### STUN and TURN without a third-party account

**There is no default STUN or TURN server.** Two tabs on one machine, or two laptops on one
network, reach each other directly, so `k dev` needs nothing. A deployment that must connect
across networks runs its own **coturn** next to the app, and the app's `IceServers` service hands
browsers short-lived credentials for it, signed with a secret only the server knows:

```k
// examples/call/src/calls/CallApi.k
    rpc iceServers() async throws RpcOnlyError -> List<IceServer> {
        return await ice.forClient(user: session.user.id.name, validFor: 1.hours)
    }
```

- `IceServers` reads the stdlib's **`TurnConfig`**: `K_TURN_URLS` (`turn:` URLs, comma-separated)
  and `K_TURN_SECRET` (coturn's `static-auth-secret`, read from `K_TURN_SECRET_FILE` in
  containers). Without URLs it issues none. URLs without a secret, or a URL whose scheme isn't
  `stun`, `turn` or `turns`, stop the server at startup (exit 78).
- The credentials follow coturn's TURN REST scheme (`--use-auth-secret`): the user name
  `<expiry>:<user>` and an HMAC of it, so a leaked credential is worth an hour of relaying.
- **Deny the private ranges** in coturn's flags, or a TURN server relays into the deployment's own
  network. The call example's `compose.yaml` does, and its deployment test checks it.
- `IcePolicy.relayOnly` sends everything through TURN: for testing a deployment's TURN server, and
  for users who must not reveal their address to the other side.

### Testing calls

`k test` puts **`TestPeerConnections`** in place of the browser's: the real negotiation, over the
app's real channel and relay, with an in-process media engine. The other side of a call is a
`TestCallPeer`, which joins as a second browser ([`TestPeers`](channels.md)) through the app's own
protocol, so the guards, the handler and the relay all run:

```k
// examples/call/src/calls/Room.test.k
    test "two people see each other and chat" {
        await sessions.signIn(userId: AccountId(name: "ana"))
        let screen = render(Room(id: callRoom))
        screen.button(named: "Join call").click()
        await screen.settle()
// ...
        let sam = await peers.signIn(userId: AccountId(name: "sam"))
        let samsApi: CallProtocol = sam.client()
        let other = try await rtc.peer(joining: try await samsApi.join(room: callRoom))
        other.send(devices.camera.newStream())
        try await other.channel(introductions).send(Introduction(name: "Sam"))
        await screen.settle()

        expect not screen.status().exists
        expect screen.video(labeled: "Sam's camera").isPlaying
```

- `other.send(_:)`, `other.channel(_:)` and `other.close()` act as the other person;
  `other.received(chatLines)` lists what the app sent on a channel, and `other.remote` is what the
  app sends, whose tracks are the app's own, so a test checks that muting disabled them.
- `rtc.simulate(_:)` changes the in-process network: `.networkLost`, `.networkRestored`,
  `.iceFails`, or `.glare`, both sides offering at once. Timeouts follow the `TestClock`.
- `devices.camera.newStream()` is a stream of a test device that needs no permission, for the other
  person to send; `screen.video(labeled:)` and `screen.audio(labeled:)` find media elements, with
  `isPlaying` and `isMuted`.
- `IceServers` runs for real, with `TurnConfig`'s built-in test value (no URLs), so `iceServers()`
  answers `[]`.
