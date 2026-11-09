import k.crypto.Uuid

/// A memo without its audio, as the list shows it.
public struct MemoSummary implements Codable, Equatable {
    /// The memo's id.
    let id: Uuid
    /// When the recording started.
    let recordedAt: Date
    /// How long it is.
    let duration: Duration
    /// The recording's media type.
    let mediaType: String
    /// The recording's size in bytes.
    let size: Int

    /// The length as `m:ss`: `0:03`, `2:47`.
    func length() -> String {
        return clockText(duration)
    }

    /// The file name a download gets: `memo-<id>.webm` or `.mp4`.
    func fileName() -> String {
        if mediaType.contains("mp4") {
            return "memo-{id}.mp4"
        }
        return "memo-{id}.webm"
    }
}
