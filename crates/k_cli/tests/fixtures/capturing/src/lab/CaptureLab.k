import k.device.PermissionState
import k.media.AudioCapture
import k.media.CaptureError
import k.media.CaptureRequest
import k.media.MediaDeviceId
import k.media.MediaDevices
import k.media.MediaStream
import k.media.Recorder
import k.media.Recording
import k.media.RecordingFormat
import k.time.Clock

/// Captures and records one way per button, and says what happened.
public service CaptureLab {
    @Inject let devices: MediaDevices
    @Inject let recorder: Recorder
    @Inject let clock: Clock

    /// What the last button did.
    @State var status: String = "ready"

    /// The last recording.
    @State var recording: Recording? = nil

    /// Whether this browser can capture here.
    @Derived let isSupported: Bool = devices.isSupported

    /// Asks for the camera and the microphone.
    func ask() async {
        let answer = await devices.requestAccess(for: .cameraAndMicrophone)
        status = "access {permissionName(answer)}"
    }

    /// Captures the default microphone, then stops it.
    func capture() async {
        await open(.microphone)
    }

    /// Captures a microphone that isn't there.
    func captureMissing() async {
        await open(.audio(AudioCapture(device: MediaDeviceId(value: "no-such-microphone"))))
    }

    /// Records two seconds in `format` and keeps the recording.
    func record(format: RecordingFormat) async {
        recording = nil
        var request: CaptureRequest = .microphone
        if format == .video {
            request = .cameraAndMicrophone
        }
        var captured: MediaStream? = nil
        do {
            captured = try await devices.capture(request)
        } catch let error {
            status = "capture failed: {errorName(error)}"
        }
        guard let stream = captured else { return }

        do {
            let session = try recorder.start(stream, format: format)
            await clock.sleep(2.seconds)
            let finished = try await session.stop()
            recording = finished
            status = "recorded {finished.file.mediaType} {finished.file.name}"
        } catch .unsupportedFormat {
            status = "record failed: unsupportedFormat"
        } catch .noTrack {
            status = "record failed: noTrack"
        } catch .failed {
            status = "record failed: failed"
        }
    }

    /// Captures `request` and reports the outcome.
    private func open(_ request: CaptureRequest) async {
        do {
            let stream = try await devices.capture(request)
            status = "captured {stream.audioTracks.count} audio"
            stream.stop()
        } catch let error {
            status = "capture failed: {errorName(error)}"
        }
    }

    /// A permission's name.
    private func permissionName(_ permission: PermissionState) -> String {
        return match permission {
            case .prompt { "prompt" }

            case .granted { "granted" }

            case .denied { "denied" }
        }
    }

    /// A capture error's case.
    private func errorName(_ error: CaptureError) -> String {
        return match error {
            case .unsupported { "unsupported" }

            case .notAsked { "notAsked" }

            case .denied { "denied" }

            case .cancelled { "cancelled" }

            case .notFound { "notFound" }

            case .busy { "busy" }

            case .unsatisfiable(let setting) { "unsatisfiable {setting}" }
        }
    }
}
