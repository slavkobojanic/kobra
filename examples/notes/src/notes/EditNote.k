import k.auth.Authenticated
import k.crypto.Uuid
import k.validation.ValidationError

/// Edits one note, and resolves conflicts with edits made in another tab.
///
/// Every save names the revision it started from. When someone else saved
/// in the meantime, nothing is overwritten: the page keeps the person's text
/// and asks whose version to keep.
public page EditNote requires Authenticated {
    /// The note to edit. A value that isn't a UUID gives a 404.
    @Param let id: Uuid

    @Inject let notes: NotesService

    /// The note as it was when the page opened.
    @Load let loaded: NoteSummary = try await notes.get(id: id)

    /// The newest version this page knows of, once a save or a conflict has
    /// replaced `loaded`.
    @State var latest: NoteSummary? = nil

    /// A save that lost to a newer version, waiting for the person to choose.
    @State var conflict: EditConflict? = nil

    /// Whether the last save succeeded, for the status message.
    @State var isSaved: Bool = false

    /// The version the next save is based on.
    @Derived let current: NoteSummary = latest ?? loaded

    /// Saves a validated draft on top of `current`. A validation failure
    /// from the server is passed back to the editor's form.
    func save(draft: NoteDraft) async throws ValidationError {
        isSaved = false

        do {
            latest = try await notes.update(note: current, draft: draft)
            conflict = nil
            isSaved = true
        } catch .conflict(let newer) {
            conflict = EditConflict(theirs: newer, mine: draft)
        } catch .notFound {
            Router.navigate(to: .notes)
        } catch .rpcFailure(.invalid(let failure)) {
            throw failure
        } catch .rpcFailure(.rejected) {
            Router.navigate(to: .signIn.returnTo(.editNote(id: id)))
        }
    }

    /// Resolves a conflict by saving this tab's text over the newer version.
    func keepMine() async {
        guard let pending = conflict else {
            return
        }

        latest = pending.theirs

        do {
            try await save(draft: pending.mine)
        } catch _ {
            // Unreachable in practice: the server reported the conflict from
            // inside the handler, so this draft has already passed its
            // validation.
        }
    }

    /// Deletes the note and returns to the list.
    func remove() async {
        do {
            try await notes.delete(note: current)

            Router.navigate(to: .notes)
        } catch .rpcFailure {
            Router.navigate(to: .signIn.returnTo(.editNote(id: id)))
        }
    }

    metadata {
        title("{current.title} · K Notes")
    }

    view {
        heading(level: 1) { "Edit note" }

        if let conflict {
            aside {
                paragraph { "This note was changed in another tab while you were editing. The other version is titled “{conflict.theirs.title}”." }

                button { "Keep my version" }
                    .on(click: keepMine)

                link(to: .notes) { "Discard my changes" }
            }
            .class(.conflict)
            .role(.alert)
        }

        NoteEditor(draft: loaded.draft(), save: save)

        if isSaved {
            paragraph { "Saved." }
                .class(.status)
                .role(.status)
        }

        button { "Delete note" }
            .class(.delete)
            .on(click: remove)
    }

    loading {
        paragraph { "Loading note…" }
            .role(.status)
    }

    failed(let error) {
        match error {
            case .notFound {
                metadata {
                    status(.notFound)
                }

                paragraph { "This note doesn't exist, or it was deleted." }

                link(to: .notes) { "Back to notes" }
            }

            case .rpcFailure {
                paragraph { "Your session has ended." }

                link(to: .signIn.returnTo(.editNote(id: id))) { "Sign in again" }
            }
        }
    }

    style {
        .conflict {
            display: grid;
            gap: .sm;
            marginBottom: .md;
            padding: .md;
            border: 1px solid .highlight;
            borderRadius: .md;
        }

        .status { color: .muted; }
        .delete { marginTop: .lg; color: .danger; }
    }
}

/// A save that lost to a newer version of the note.
struct EditConflict {
    /// The version saved elsewhere.
    let theirs: NoteSummary

    /// What this tab tried to save.
    let mine: NoteDraft
}
