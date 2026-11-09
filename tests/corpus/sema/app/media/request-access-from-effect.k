// `MediaDevices.requestAccess(for:)` is the only camera and microphone prompt, and
// `captureScreen` shows a picker: both are `@UserAction` (K4030 from an effect, or after an
// `await`), while `capture` never prompts and may run anywhere in the browser (46, "Capture").
import k.media.MediaDevices
import k.media.MediaStream

service CallService {
    @Inject let devices: MediaDevices

    @State var camera: MediaStream? = nil

    func join() async {
        await devices.requestAccess(for: .cameraAndMicrophone)
        do {
            camera = try await devices.capture(.camera)
        } catch _ {
            camera = nil
        }
    }

    func shareLater() async {
        await devices.requestAccess(for: .microphone)
        do {
            camera = try await devices.captureScreen()
        } catch _ {
            camera = nil
        }
    }

    func resume() async {
        do {
            camera = try await devices.capture(.camera)
        } catch _ {
            camera = nil
        }
    }
}

component CallPage {
    @Inject let call: CallService

    @Effect
    func start() async {
        await call.join()
    }

    @Effect
    func reopen() async {
        await call.resume()
    }

    view {
        button { "Join" }
            .on(click: { await call.join() })
    }
}
