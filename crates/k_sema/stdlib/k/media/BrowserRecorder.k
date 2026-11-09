/// The standard [`Recorder`]: the browser's `MediaRecorder`.
@ClientOnly
@Native
public service BrowserRecorder implements Recorder {
    /// See [`Recorder.start(_:format:limit:)`].
    func start(_ stream: MediaStream, format: RecordingFormat, limit: RecordingLimit = RecordingLimit()) throws RecordingError -> RecordingSession
    /// See [`Recorder.supports(_:)`].
    func supports(_ format: RecordingFormat) -> Bool
}
