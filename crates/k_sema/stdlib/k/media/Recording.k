import k.device.File

/// A finished recording: a [`k.device.File`] to play, save or upload.
public struct Recording implements Equatable {
    /// `recording.webm` or `recording.mp4`, with its media type.
    let file: File
    /// How long it is.
    let duration: Duration
    /// The format it was recorded in.
    let format: RecordingFormat
}
