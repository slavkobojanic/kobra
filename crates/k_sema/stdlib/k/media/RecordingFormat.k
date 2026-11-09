/// What a recording is encoded as. The named formats try their WebM form,
/// then their MP4 form, so Chromium and Firefox record WebM and Safari
/// MP4.
public enum RecordingFormat implements Equatable {
    /// Speech: Opus at 32 kbps in WebM, else AAC in MP4.
    case voice
    /// Music: Opus at 128 kbps in WebM, else AAC in MP4.
    case audio
    /// VP9 or VP8 with Opus in WebM, else H.264 with AAC in MP4; 2.5 Mbps.
    case video
    /// Exactly this media type and these bit rates.
    case exact(mediaType: String, audioBitsPerSecond: Int?, videoBitsPerSecond: Int?)
}
