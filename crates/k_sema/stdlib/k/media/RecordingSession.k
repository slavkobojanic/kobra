/// A recording in progress. Its fields are reactive.
@ClientOnly
@Native
public class RecordingSession {
    /// `.recording`, `.paused`, or `.stopped` and why.
    var state: RecordingState
    /// The time recorded, paused time excluded; updated each second.
    var elapsed: Duration
    /// The bytes recorded so far.
    var size: Int

    /// Pauses the recording.
    func pause()
    /// Resumes a paused recording.
    func resume()
    /// Finishes the file and, unless `keepStream` is `true`, stops the
    /// stream. After the recording stopped by itself it returns everything
    /// recorded until then.
    ///
    /// - Throws: [`RecordingError.failed`] when the browser's encoder
    ///   failed.
    func stop(keepStream: Bool = false) async throws RecordingError -> Recording
}
