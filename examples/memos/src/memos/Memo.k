import k.crypto.Uuid

/// A voice memo, stored in the `memo` table with its audio.
///
/// Server-only, like every entity. The browser lists `MemoSummary` values
/// and fetches a memo's audio only when its row shows it.
public entity Memo {
    /// The primary key. Random, so memo URLs can't be counted.
    @Id let id: Uuid

    /// When the recording started, from the browser's clock.
    @Index let recordedAt: Date

    /// How long it is, in milliseconds.
    let durationMilliseconds: Int

    /// What the browser recorded: `audio/webm` in Chromium and Firefox,
    /// `audio/mp4` in Safari.
    let mediaType: String

    /// The recording's size in bytes.
    let size: Int

    /// The recording itself, as Base64 text: an entity column can't hold
    /// `Bytes` yet. A memo fits in one rpc request (1 MB), so it is stored
    /// whole.
    let audioBase64: String
}
