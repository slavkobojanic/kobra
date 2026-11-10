/// The newest edits this browser kept because they couldn't be saved, each
/// a link back to its note, through the `savedAt` index.
@Rendering(.client)
public component UnsavedEditsList {
    @Inject let offline: OfflineEdits

    @Effect
    func load() async {
        await offline.loadNewest()
    }

    view {
        if not offline.newest.isEmpty {
            section {
                heading(level: 2) { "Unsaved edits" }

                list {
                    for edit in offline.newest key edit.noteId {
                        item {
                            link(to: .editNote(id: edit.noteId)) { edit.title }
                        }
                    }
                }
            }
            .class(.unsaved)
        }
    }

    loading {
    }

    style {
        .unsaved { marginBottom: .md; }
    }
}
