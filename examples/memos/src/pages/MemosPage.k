import memos.MemoRow
import memos.MemoService
import recording.RecorderPanel

/// The memos: the recorder above, and every memo below, newest first.
public page MemosPage at "/" {
    @Inject let memos: MemoService

    /// Whether the list couldn't be loaded.
    @State var isDisconnected: Bool = false

    /// Loads the list once the page runs in the browser.
    @Effect
    func loadMemos() async {
        do {
            try await memos.load()
        } catch .rpcFailure {
            isDisconnected = true
        }
    }

    metadata {
        title("Memos")
        description("Record voice memos and play them back.")
    }

    view {
        main {
            heading(level: 1) { "Memos" }

            RecorderPanel()

            if isDisconnected {
                paragraph { "Couldn't reach the server to load your memos." }
                    .role(.alert)
            } else if memos.isLoaded and memos.items.isEmpty {
                paragraph { "No memos yet. Press Record to make one." }
            } else {
                list {
                    for memo in memos.items key memo.id {
                        item {
                            MemoRow(memo: memo)
                        }
                    }
                }
                .label("Memos")
            }
        }
        .class(.page)
    }

    style {
        .page {
            display: grid;
            gap: .md;
            maxWidth: 40rem;
            margin: 0 auto;
            padding: .lg;
        }
    }
}
