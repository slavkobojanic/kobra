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

    /// Whether the change stream couldn't be opened because the server was
    /// out of reach.
    @State var isDisconnected: Bool = false

    /// Goes up by one each time the person asks to reconnect, which re-runs
    /// `followChanges`.
    @State var connectAttempt: Int = 0

    /// Why the last pin toggle failed, for the alert.
    @State var problem: String? = nil

    /// Follows the account's change stream while the page is open. It runs
    /// after the page mounts and again on each reconnect, in the browser
    /// only; leaving the page cancels it, which closes the stream.
    ///
    /// When the account signs out, here or in another tab, the session ends
    /// and so does the stream. Its end tells the browser the session is
    /// gone, so the router leaves this page, which requires a signed-in
    /// user, before `NotesService.watch()` can open a new one. The
    /// `.rejected` case below covers a refusal the browser's session copy
    /// didn't foresee.
    @Effect(on: [connectAttempt])
    func followChanges() async {
        isDisconnected = false

        do {
            try await notes.watch()
        } catch .rpcFailure(.rejected) {
            Router.navigate(to: .signIn.returnTo(.notes))
        } catch .rpcFailure {
            isDisconnected = true
        }
    }

    /// Opens the change stream again after a network failure.
    func reconnect() {
        connectAttempt += 1
    }

    /// Pins or unpins a note from its card.
    func togglePin(note: NoteSummary) async {
        problem = nil

        do {
            try await notes.togglePin(note: note)
        } catch .notFound {
            // Deleted in another tab. The service has already dropped it.
        } catch .rpcFailure(.rejected) {
            Router.navigate(to: .signIn.returnTo(.notes))
        } catch .rpcFailure {
            problem = "Couldn't reach K Notes, so “{note.title}” wasn't changed. Try again."
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

        UnsavedEditsList()

        textField(value: bind filter)
            .label("Filter")
            .placeholder("Search titles, text and tags")

        if isDisconnected {
            aside {
                paragraph { "Couldn't reach K Notes to load your notes." }

                button { "Reconnect" }
                    .on(click: reconnect)
            }
            .class(.problem)
            .role(.alert)
        }

        if let problem {
            paragraph { problem }
                .class(.problem)
                .role(.alert)
        }

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
        } else if not isDisconnected {
            paragraph { "Loading notes…" }
                .class(.hint)
                .role(.status)
        }
    }

    style {
        .toolbar { display: flex; alignItems: center; justifyContent: spaceBetween; marginBottom: .md; }
        .newNote { padding: .sm .md; borderRadius: .md; backgroundColor: .primary; color: .surface; textDecoration: none; }
        .hint    { color: .muted; }
        .problem { display: flex; alignItems: center; gap: .md; marginBottom: .md; color: .danger; }
        .notes   { display: grid; gap: .md; marginTop: .md; padding: 0; listStyle: none; }
    }
}
