/// Memos: a voice memo recorder (`docs/design/46-media-and-rtc.md`,
/// "Example apps").
///
/// There are no `use … for …` rules. `MediaDevices`, `Recorder` and `Audio`
/// are client-only platform interfaces with one production implementation
/// each, so they are injected with no rule, and `k test` replaces them with
/// `TestMediaDevices`, `TestRecorder` and `TestAudio`: no prompt appears and
/// no microphone opens in a test. `Database` connects with `DatabaseConfig`
/// to `DATABASE_URL` (a SQLite file in development), and to a fresh
/// in-memory database for each `k test` run.
public app Memos {
    metadata {
        manifest(name: "Memos", shortName: "Memos")
        themeColor(.primary)
        locale(.english)
    }
}
