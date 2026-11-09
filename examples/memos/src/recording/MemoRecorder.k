import k.device.PermissionState
import k.media.Audio
import k.media.AudioCapture
import k.media.AudioMeter
import k.media.MediaDevice
import k.media.MediaDeviceId
import k.media.MediaDevices
import k.media.MediaStream
import k.media.Recorder
import k.media.Recording
import k.media.RecordingLimit
import k.media.RecordingSession
import k.media.RecordingStop
import k.storage.LocalStorage
import k.storage.StorageKey
import k.time.Clock
import memos.MemoService

/// Records a memo from the microphone and stores it
/// (`docs/design/46-media-and-rtc.md`).
///
/// It injects the client-only `MediaDevices`, `Recorder`, `Audio` and
/// `LocalStorage`, so it is client-only; `RecorderPanel`, which shows it, is
/// rendered in the browser. Its `.route` scope, and the `.service` lifetime
/// of its capture, keep the microphone open across the panel's re-renders
/// and close it when the page is left.
@Scope(.route)
public service MemoRecorder {
    @Inject let devices: MediaDevices
    @Inject let recorder: Recorder
    @Inject let audio: Audio
    @Inject let memos: MemoService
    @Inject let storage: LocalStorage
    @Inject let clock: Clock

    // Recorded in storage.lock, so a change that would make the saved
    // choice unreadable fails `k check`.
    private let preferredMicrophone: StorageKey<MediaDeviceId> = StorageKey(name: "microphone")

    /// The page shows a Record button, or how to unblock the microphone.
    @Derived let microphone: PermissionState = devices.microphonePermission

    /// Whether this browser can record here at all.
    @Derived let canRecord: Bool = devices.isSupported

    /// The microphones to choose from; labelled once access is granted.
    @Derived let microphones: List<MediaDevice> = devices.devices.filter { d => d.kind == .microphone }

    /// The chosen microphone, remembered in local storage; `nil` is the
    /// browser's default.
    @State var chosen: MediaDeviceId? = nil

    /// The recording in progress.
    @State var session: RecordingSession? = nil

    /// The microphone's level while recording.
    @State var meter: AudioMeter? = nil

    /// Why the last recording failed.
    @State var problem: MemoProblem? = nil

    /// What happened to a recording that stopped by itself.
    @State var notice: String? = nil

    /// When the recording in progress started.
    private var startedAt: Date? = nil

    /// Restores the chosen microphone. In the browser this runs after
    /// hydration.
    init() {
        do {
            chosen = try storage.read(key: preferredMicrophone)
        } catch .unreadable, .unavailable {
            chosen = nil
        }
    }

    /// Chooses a microphone by its id, and remembers it.
    func choose(id: String) {
        let device = MediaDeviceId(value: id)
        chosen = device
        do {
            try storage.write(key: preferredMicrophone, value: device)
        } catch .quotaExceeded, .unavailable {
            // Remembering is a convenience; recording works without it.
        }
    }

    // Called from the Record button's click handler, so it may prompt.
    /// Asks for the microphone the first time, then records.
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

        do {
            meter = try audio.meter(for: stream, lifetime: .service)
        } catch .unsupported, .noTrack {
            meter = nil
        }

        do {
            // Three minutes of speech fit in one 1 MB rpc request.
            session = try recorder.start(stream, format: .voice, limit: RecordingLimit(size: 750000, duration: 3.minutes))
            startedAt = clock.now
        } catch let failure {
            stream.stop()
            meter = nil
            problem = .cannotRecord(failure)
        }
    }

    /// Pauses or resumes the recording.
    func togglePause() {
        guard let current = session else { return }
        if current.state == .paused {
            current.resume()
        } else {
            current.pause()
        }
    }

    /// Why the recording stopped by itself; `nil` while it runs.
    func stoppedBy() -> RecordingStop? {
        guard let current = session else { return nil }
        if current.state == .stopped(.streamEnded) {
            return .streamEnded
        }
        if current.state == .stopped(.limitReached) {
            return .limitReached
        }
        return nil
    }

    /// Finishes the recording, which stops the microphone too, and stores
    /// it. After a recording that stopped by itself, it says why.
    func stop() async {
        guard let current = session else { return }
        let reason = stoppedBy()
        session = nil
        meter = nil

        var finished: Recording? = nil
        do {
            finished = try await current.stop()
        } catch let failure {
            problem = .cannotRecord(failure)
        }
        guard let memo = finished else { return }

        do {
            try await memos.upload(memo, recordedAt: startedAt ?? clock.now)
            if reason == .streamEnded {
                notice = "The microphone was disconnected, so recording stopped. The memo was saved."
            } else if reason == .limitReached {
                notice = "Memos are at most three minutes long, so recording stopped. The memo was saved."
            }
        } catch .notAMemo, .rpcFailure {
            problem = .notSaved
        }
    }
}
