# Memos

A voice memo recorder written entirely in K: press **Record**, watch the microphone's level, pause and resume, stop, and the memo is stored on the server and appears in a list where it plays, downloads and deletes. Data lives in SQLite. It needs no account and no external server.

It is the example of `k.media` ([`docs/design/46-media-and-rtc.md`](../../docs/design/46-media-and-rtc.md)): capturing the microphone with a permission that is a value, recording to a `File`, a live level meter, and playing a recording back, with every part tested against hermetic doubles.

## Running it

| Command, here | What it does |
|---|---|
| `k dev` | Builds and serves the app on `http://localhost:8080`, with a SQLite file at `.k/memos.db`, created and migrated at startup. |
| `k test` | Runs the tests against `TestMediaDevices`, `TestRecorder` and `TestAudio`: no prompt appears and no microphone opens. |
| `k check` | Checks the code, the lockfiles and the tests. |
| `k build` | Builds `dist/`, whose `dist/app` serves the page and the rpcs. |

Browsers allow the microphone only in a secure context: `http://localhost` (what `k dev` serves) or `https`. Opened from another machine over plain `http`, the page says the browser can't record here.

## What it demonstrates

| Feature | Where |
|---|---|
| `MediaDevices`: the microphone's permission as a reactive value, `requestAccess(for:)` as the only prompt (from the Record button's click), and `capture(_:lifetime:)` with every `CaptureError` handled | `src/recording/MemoRecorder.k` |
| A view for every permission state: a Record button for `.prompt` and `.granted`, how to unblock the microphone for `.denied`, and a notice where the browser can't capture | `src/recording/RecorderPanel.k` |
| A microphone picker from `devices`, its choice remembered in `LocalStorage` under a key recorded in `storage.lock` | `src/recording/MemoRecorder.k`, `src/recording/RecorderPanel.k`, `storage.lock` |
| `Recorder`: recording `.voice` with a `RecordingLimit`, pause and resume, and a recording that stops by itself when the microphone is unplugged | `src/recording/MemoRecorder.k`, `src/recording/RecorderPanel.k` |
| `Audio` and `LevelMeter`: the microphone's level while recording | `src/recording/MemoRecorder.k`, `src/recording/RecorderPanel.k` |
| A client-rendered component (`@Rendering(.client)`) for what only the browser knows | `src/recording/RecorderPanel.k` |
| The `audio` element playing a `File`, and `Files.save` for a download | `src/memos/MemoRow.k`, `src/memos/MemoDownloads.k` |
| A protocol carrying a recording's bytes, an entity storing them and a migration | `src/memos/MemosProtocol.k`, `src/memos/MemosApi.k`, `src/memos/Memo.k`, `migrations/0001_create_memos.k` |
| Page tests with the media doubles: a blocked microphone, a recorded memo, a refused prompt, paused time, an unplugged microphone, and leaving the page turning the microphone off | `src/pages/MemosPage.test.k` |

## How it works

### Asking for the microphone

`MemoRecorder.start()` runs from the Record button's click. While the microphone's permission is `.prompt` it calls `requestAccess(for: .microphone)`, the only member that shows the browser's prompt; the compiler allows it only from a user's action, before the handler's first `await`. Then it captures with `capture(_:lifetime:)`, which never prompts: it throws `.notAsked` or `.denied` when the user hasn't allowed the microphone, and every other `CaptureError` case is handled too, so the page always says what happened.

`RecorderPanel` reads `MemoRecorder.microphone`, a `@Derived` value of the reactive permission, so changing the site's settings in the browser updates the page at once. It is rendered in the browser, since the server can't know the permission or the devices.

### Recording and storing

The capture uses `lifetime: .service`: the stream belongs to the route-scoped `MemoRecorder`, so it stays open across the panel's re-renders and stops when the page is left. The recording is `.voice` (Opus in WebM in Chromium and Firefox, AAC in MP4 in Safari) with a limit of three minutes or 750 kB: a memo travels whole in one rpc request, whose limit is 1 MB once its bytes are Base64-encoded in JSON.

`stop()` finishes the file, which stops the microphone too, and uploads it with `MemosProtocol.upload`. The server checks the length and the media type and stores the bytes in SQLite, as Base64 text, since an entity column can't hold `Bytes` yet; it never decodes the audio. Each row fetches its memo's recording when it appears and plays it in an `audio` element.

When the microphone is unplugged mid-recording, the recording stops by itself with `.stopped(.streamEnded)`; `RecorderPanel`'s effect then stores what was recorded and says so. Reaching the limit works the same way.

### Tests

`k test` gives the app `TestMediaDevices`, `TestRecorder` and `TestAudio`. The tests click Record, script the microphone with `devices.microphone.speak(level:for:)`, move the `TestClock`, and check what the page shows and that `devices.liveTracks` is empty once the recording, or the page, is gone.

## Design documents used

| Document | For |
|---|---|
| [46-media-and-rtc.md](../../docs/design/46-media-and-rtc.md) | capture, recording, levels, the `audio` element, the doubles, and this example |
| [45-web-platform.md](../../docs/design/45-web-platform.md) | `PermissionState`, `@UserAction`, client-only interfaces during server rendering |
| [42-files.md](../../docs/design/42-files.md) | `File` and `Files.save` |
| [47-client-rendered-components.md](../../docs/design/47-client-rendered-components.md) | `@Rendering(.client)` |
| [36-local-storage.md](../../docs/design/36-local-storage.md) | the remembered microphone |
| [19-persistence.md](../../docs/design/19-persistence.md) | the `memo` entity and its migration |
