import k.crypto.Uuid
import k.validation.ValidationError

/// Offers the unsaved edit of a note, if this browser kept one: save it now
/// or discard it.
///
/// The edit lives in this browser's IndexedDB, so the component renders in
/// the browser only: the server renders its empty `loading` block.
@Rendering(.client)
public component UnsavedEditNotice {
    /// The note being edited.
    @Prop let noteId: Uuid

    /// Saves a draft, as the edit page's own form does.
    @Prop let save: (NoteDraft) async throws ValidationError -> Void

    @Inject let offline: OfflineEdits

    @State var edit: UnsavedEdit? = nil

    @Effect(on: [noteId])
    func load() async {
        edit = await offline.edit(of: noteId)
    }

    func saveNow() async {
        guard let pending = edit else {
            return
        }

        do {
            try await save(pending.draft())
        } catch _ {
            // The page shows why; the edit stays offered.
        }
        edit = await offline.edit(of: noteId)
    }

    func discard() async {
        await offline.discard(note: noteId)
        edit = nil
    }

    view {
        if let edit {
            aside {
                paragraph { "This browser kept an edit of this note that couldn't be saved: “{edit.title}”." }

                button { "Save that edit" }
                    .on(click: saveNow)

                button { "Discard it" }
                    .on(click: discard)
            }
            .class(.notice)
            .role(.status)
        }
    }

    loading {
    }

    style {
        .notice {
            display: grid;
            gap: .sm;
            marginBottom: .md;
            padding: .md;
            border: 1px solid .border;
            borderRadius: .md;
        }
    }
}
