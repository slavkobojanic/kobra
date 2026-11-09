import k.media.MediaDevices
import k.media.MediaStream
import k.media.Recorder
import k.media.RecordingSession

@Scope(.route)
public service MemoRecorder {
    @Inject let devices: MediaDevices
    @Inject let recorder: Recorder

    @State var session: RecordingSession? = nil
    @State var stream: MediaStream? = nil

    func start() async {
        await devices.requestAccess(for: .microphone)
        do {
            let s = try await devices.capture(.microphone, lifetime: .service)
            stream = s
            session = try recorder.start(s, format: .voice)
        } catch _ {
            session = nil
        }
    }
}
