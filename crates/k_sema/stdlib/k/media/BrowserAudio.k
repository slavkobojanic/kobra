/// The standard [`Audio`]: one `AudioContext` per app.
@ClientOnly
@Native
public service BrowserAudio implements Audio {
    /// See [`Audio.meter(for:lifetime:)`].
    func meter(for stream: MediaStream, lifetime: CaptureLifetime = .owner) throws AudioError -> AudioMeter
    /// See [`Audio.play(_:)`].
    func play(_ source: MediaSource) async throws PlaybackError
}
