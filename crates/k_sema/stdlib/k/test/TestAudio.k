import k.media.Audio
import k.media.AudioError
import k.media.AudioMeter
import k.media.CaptureLifetime
import k.media.MediaSource
import k.media.MediaStream
import k.media.PlaybackError

/// The default [`Audio`] in `k test`: a meter reports the level its
/// stream's [`TestSource`] scripts, and `play` records what it played.
@TestDefault(replaces: Audio)
@Native
public service TestAudio implements Audio {
    /// Every source given to `play`, oldest first.
    var played: List<MediaSource>

    /// A meter reading the scripted level of the stream's microphone.
    func meter(for stream: MediaStream, lifetime: CaptureLifetime = .owner) throws AudioError -> AudioMeter
    /// Records `source` in [`played`] and resolves at once.
    func play(_ source: MediaSource) async throws PlaybackError
}
