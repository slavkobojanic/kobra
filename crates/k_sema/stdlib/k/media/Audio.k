/// Audio levels and short sounds (`docs/design/46-media-and-rtc.md`,
/// "Audio"). A client-only platform interface: [`BrowserAudio`] is
/// injected with no `use` rule, and [`k.test.TestAudio`] replaces it in
/// tests.
@Platform(side: .client)
public interface Audio {
    /// Measures the loudness of `stream`'s sound. It reads `0.0` until the
    /// first user's action on the page, when browsers let audio start.
    ///
    /// - Throws: [`AudioError.noTrack`] for a stream without audio,
    ///   `.unsupported` where Web Audio is missing.
    func meter(for stream: MediaStream, lifetime: CaptureLifetime = .owner) throws AudioError -> AudioMeter

    /// Plays a short sound without an element, and resolves when it ends.
    func play(_ source: MediaSource) async throws PlaybackError
}
