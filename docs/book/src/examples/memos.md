# Memos: recording voice memos

`examples/memos` is a voice memo recorder: press Record, watch the microphone's level, pause and
resume, stop, and the memo is stored on the server and appears in a list where it plays, downloads
and deletes. It is the example of [camera, microphone and recording](../apps/media.md), and needs
no account and no external server.

```text
memos/
├── App.k, k.toml, storage.lock
├── migrations/0001_create_memos.k
└── src/
    ├── memos/      the stored memos: Memo, MemoRepository, MemosProtocol, MemosApi, MemoService, MemoRow, MemoDownloads
    ├── recording/  the microphone: MemoRecorder, RecorderPanel, MemoProblem
    ├── pages/      MemosPage and its tests
    └── shell/      MemosTheme
```

## Running it

| Command, at `examples/memos` | What it does |
|---|---|
| `k dev` | Builds and serves the app on port 8080 with a SQLite file at `.k/memos.db`. Open `http://localhost:8080`: capture works on `localhost` and over `https` only. |
| `k test` | Runs the tests against `TestMediaDevices`, `TestRecorder` and `TestAudio`: no prompt appears and no microphone opens. |
| `k build` | Builds `dist/`, whose `dist/app` serves the page and the rpcs. |

## Every permission state

`RecorderPanel` shows what the microphone's permission allows. It is rendered in the browser
(`@Rendering(.client)`), since only the browser knows the permission and the devices:

```k
// examples/memos/src/recording/RecorderPanel.k
        if not recorder.canRecord {
            paragraph { "This browser can't record here. Open Memos over https, or on localhost." }
                .role(.alert)
        } else if recorder.microphone == .denied {
            paragraph { "Microphone access is blocked. Allow it in your browser's site settings to record." }
                .role(.alert)
```

Otherwise it shows a Record button, and a microphone picker when there is more than one. The choice
is kept in `LocalStorage` under a key recorded in `storage.lock`. The first click asks for the
microphone; a refusal shows the same explanation.

## Recording and storing a memo

`MemoRecorder` captures with `lifetime: .service`, so the microphone stays open across the panel's
re-renders and closes when the page is left. It records `.voice` with a limit of three minutes or
750 kB, which fits one rpc request. `stop()` finishes the file, which stops the microphone too, and
uploads it:

```k
// examples/memos/src/memos/MemoService.k
    func upload(_ recording: Recording, recordedAt: Date) async throws UploadError {
        let memo = try await api.upload(recordedAt: recordedAt, duration: recording.duration, mediaType: recording.file.mediaType, audio: recording.file.bytes)
        items = [memo] + items
    }
```

The server stores the bytes as Base64 text in SQLite; it can't decode or inspect media. When the
microphone is unplugged mid-recording, the recording stops by itself and `RecorderPanel`'s effect
stores what was recorded and says so.

## Tests

The page's tests are the ones the design shows, and three more: a refused prompt, paused time left
out, and an unplugged microphone.

```k
// examples/memos/src/pages/MemosPage.test.k
    test "leaving the page while recording turns the microphone off" {
        let screen = render(MemosPage())

        screen.button(named: "Record").click()
        await screen.settle()
        screen.unmount()

        expect devices.liveTracks.isEmpty
    }
```

The real-browser suite runs the example in Chrome with its fake microphone: a memo is recorded,
stored and played back from the server's copy (`crates/k_cli/tests/chrome.rs`).
