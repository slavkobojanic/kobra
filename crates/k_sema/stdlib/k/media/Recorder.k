/// Records a [`MediaStream`] to a file
/// (`docs/design/46-media-and-rtc.md`, "Recording"). A client-only
/// platform interface: [`BrowserRecorder`] is injected with no `use` rule,
/// and [`k.test.TestRecorder`] replaces it in tests.
@Platform(side: .client)
public interface Recorder {
    /// Starts recording `stream`. The session belongs to the stream's
    /// owner: it is stopped, and its data discarded, with the stream.
    ///
    /// - Parameters:
    ///   - format: `.voice`, `.audio` and `.video` always work, in WebM or
    ///     MP4 as the browser can; `.exact` names a media type.
    ///   - limit: the recording stops by itself at this size or duration.
    /// - Throws: [`RecordingError.noTrack`] when the stream has no track
    ///   the format records, `.unsupportedFormat` for an `.exact` type the
    ///   browser can't record.
    func start(_ stream: MediaStream, format: RecordingFormat, limit: RecordingLimit = RecordingLimit()) throws RecordingError -> RecordingSession

    /// Whether `format` can be recorded here; always `true` for the named
    /// formats.
    func supports(_ format: RecordingFormat) -> Bool
}
