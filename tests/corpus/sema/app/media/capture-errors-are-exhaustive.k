// A capture's `CaptureError` is handled by type: a page that assumes the user said yes is a
// missing `catch` (K2206), and an unhandled throw K2203 (46-media-and-rtc.md, "Errors").
import k.media.MediaDevices
import k.media.MediaStream

service MicService {
    @Inject let devices: MediaDevices

    @State var stream: MediaStream? = nil

    func open() async {
        do {
            stream = try await devices.capture(.microphone)
        } catch .notAsked, .denied {
            stream = nil
        } catch .notFound, .busy, .unsupported, .cancelled {
            stream = nil
        }
    }

    func openQuietly() async {
        stream = try await devices.capture(.microphone)
    }
}
