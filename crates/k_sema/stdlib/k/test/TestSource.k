/// What a test device produces, on the [`TestClock`]
/// (`docs/design/46-media-and-rtc.md`, "Tests"). Video frames are a solid
/// test pattern: tests check streams, states and labels, never pixels.
@Native
public class TestSource {
    /// Plays sound at `level` (`0.0...1.0`) for `duration`, from now on
    /// the test clock; a meter on the source reads it.
    func speak(level: Double, for duration: Duration)
    /// Plays silence for `duration`.
    func silence(for duration: Duration)
    /// Ends the source's live screen tracks with `.endedByUser`, as the
    /// browser's "Stop sharing" button does.
    func stopSharing()
}
