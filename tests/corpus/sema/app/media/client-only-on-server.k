// A stream, a recording session or a meter exists only in the browser: naming one in a
// server-only declaration is K4033 (46-media-and-rtc.md, "Server and client"). A `Recording` is a
// value and may be named anywhere.
import k.media.MediaStream
import k.media.Recording
import k.media.RecordingSession

@Scope(.server)
service Archive {
    func keep(stream: MediaStream) {
        stream.stop()
    }

    func store(recording: Recording) -> Int {
        return recording.file.bytes.count
    }

    var current: RecordingSession? = nil
}
