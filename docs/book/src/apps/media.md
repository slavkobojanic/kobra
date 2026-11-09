# Camera, microphone and recording

Capturing the camera, the microphone and the screen, recording what they produce, measuring how
loud it is and playing media back are the **`k.media`** module. Its three interfaces,
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
            await devices.requestAccess(for: .microphone)      // @UserAction: the only prompt
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

> **Planned** (k-rtc). The design adds `k.rtc`: peer-to-peer calls between two browsers with typed
> data channels, signalling over a typed channel of the app's own protocol, and self-hosted TURN
> with short-lived credentials.
