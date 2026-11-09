/// The loudness of a stream's sound. Its fields are reactive.
@ClientOnly
@Native
public class AudioMeter {
    /// The loudness now, `0.0...1.0` (RMS on a perceptual scale), updated
    /// at most 30 times a second.
    var level: Double
    /// The highest [`level`] of the last 1.5 seconds.
    var peak: Double
    /// Whether [`level`] has been above the speech threshold for 150 ms.
    var isSpeaking: Bool

    /// A snapshot of the last 50 ms, `points` values in `-1.0...1.0`, for
    /// drawing as an SVG polyline.
    func waveform(points: Int) -> List<Double>
    /// Stops measuring.
    func stop()
}
