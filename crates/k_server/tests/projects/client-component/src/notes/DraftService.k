import k.storage.LocalStorage
import k.storage.StorageKey

/// The draft note, kept in the browser's local storage: a client-only
/// service, so the server can't know it.
public service DraftService {
    @Inject let storage: LocalStorage

    private let savedDraft: StorageKey<String> = StorageKey(name: "draft")

    @State var text: String = ""

    init() {
        do {
            text = try storage.read(key: savedDraft) ?? ""
        } catch .unreadable, .unavailable {
            text = ""
        }
    }

    func update(text newText: String) {
        text = newText
        do {
            try storage.write(key: savedDraft, value: newText)
        } catch .quotaExceeded, .unavailable {
            // The draft stays in memory.
        }
    }
}
