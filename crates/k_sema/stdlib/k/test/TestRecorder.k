import k.media.MediaStream
import k.media.Recorder
import k.media.RecordingError
import k.media.RecordingFormat
import k.media.RecordingLimit
import k.media.RecordingSession

/// The default [`Recorder`] in `k test`: records a synthetic stream into a
/// small, deterministic file (100 bytes a second) whose duration follows
/// the [`TestClock`].
@TestDefault(replaces: Recorder)
@Native
public service TestRecorder implements Recorder {
    /// The number of recordings started.
    var started: Int

    /// A session whose `elapsed` follows the test clock.
    func start(_ stream: MediaStream, format: RecordingFormat, limit: RecordingLimit = RecordingLimit()) throws RecordingError -> RecordingSession
    /// `true`, unless [`failNext(with:)`] set `.unsupportedFormat`.
    func supports(_ format: RecordingFormat) -> Bool
    /// Makes the next `start` throw `error`.
    func failNext(with error: RecordingError)
}
