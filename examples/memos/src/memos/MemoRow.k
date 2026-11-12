import k.device.File

/// One memo in the list: when it was recorded and how long it is, a
/// player, a download and a delete button.
///
/// The recording is fetched when the row appears, so the list itself
/// carries no audio.
public component MemoRow {
    @Prop let memo: MemoSummary

    @Inject let memos: MemoService
    @Inject let downloads: MemoDownloads

    /// The recording, once fetched.
    @State var file: File? = nil

    /// What went wrong with this memo, for the alert.
    @State var problem: String? = nil

    /// Fetches the recording for the player.
    @Effect
    func fetch() async {
        do {
            file = try await memos.file(of: memo)
        } catch .notFound {
            problem = "This memo was deleted."
        } catch .rpcFailure {
            problem = "Couldn't load this memo. Reload the page to try again."
        }
    }

    /// Saves the recording as a download.
    func download() async {
        if not (await downloads.save(memo)) {
            problem = "Couldn't download this memo."
        }
    }

    /// Deletes the memo; the list drops it.
    func remove() async {
        do {
            try await memos.delete(memo)
        } catch .rpcFailure {
            problem = "Couldn't delete this memo. Try again."
        }
    }

    view {
        paragraph { "{memo.recordedAt.formatted(as: .dateTime)} · {memo.length()}" }
            .class(.when)

        if let file {
            audio(source: .file(file))
                .label("Memo from {memo.recordedAt.formatted(as: .time)}")
        }

        box {
            button { "Download" }
                .on(click: download)

            button { "Delete" }
                .on(click: remove)
        }
        .class(.actions)

        if let problem {
            paragraph { problem }
                .role(.alert)
        }
    }

    style {
        .when { color: .muted; }

        .actions {
            display: flex;
            gap: .sm;
        }
    }
}
