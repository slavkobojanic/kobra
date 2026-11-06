import k.auth.Authenticated

/// The signed-in account's notes, filtered as you type and kept live across
/// tabs.
///
/// This is the app's front door, so it has a named URL: `/` must keep
/// working whatever the page is called.
public page Notes requires Authenticated at "/" {
    @Inject let notes: NotesService

    @State var filter: String = ""
    @Derived let visible: List<NoteSummary> = notes.matching(filter: filter)

    /// Follows the account's change stream while the page is open. It runs
    /// once per visit, in the browser only; leaving the page cancels it,
    /// which closes the stream.
    @Effect(on: [])
    func followChanges() async {
        do {
            try await notes.watch()
        } catch .rejected {
            Router.navigate(to: .signIn.returnTo(.notes))
        }
    }

    /// Pins or unpins a note from its card.
    func togglePin(note: NoteSummary) async {
        do {
            try await notes.togglePin(note: note)
        } catch .notFound {
            // Deleted in another tab. The service has already dropped it.
        } catch .rejected {
            Router.navigate(to: .signIn.returnTo(.notes))
        }
    }

    metadata {
        title("Notes · K Notes")
    }

    view {
        header {
            heading(level: 1) { "Notes" }

            link(to: .newNote) { "New note" }
                .class(.newNote)
        }
        .class(.toolbar)

        textField(value: bind filter)
            .label("Filter")
            .placeholder("Search titles, text and tags")

        if notes.isLive {
            if notes.all.isEmpty {
                paragraph { "No notes yet. Write your first one." }
                    .class(.hint)
            } else if visible.isEmpty {
                paragraph { "No notes match “{filter}”." }
                    .class(.hint)
                    .role(.status)
            } else {
                list {
                    for note in visible key note.id {
                        item {
                            NoteCard(note: note)
                                .on(pinToggled: togglePin)
                        }
                    }
                }
                .class(.notes)
                .label("Notes")
            }
        } else {
            paragraph { "Loading notes…" }
                .class(.hint)
                .role(.status)
        }
    }

    style {
        .toolbar { display: flex; alignItems: center; justifyContent: space-between; marginBottom: .md; }
        .newNote { padding: .sm .md; borderRadius: .md; backgroundColor: .primary; color: .surface; textDecoration: none; }
        .hint    { color: .muted; }
        .notes   { display: grid; gap: .md; marginTop: .md; padding: 0; listStyle: none; }
    }
}
