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
    @Inject let offline: OfflineEdits

    /// The note as it was when the page opened.
    @Load let loaded: NoteSummary = try await notes.get(id: id)

    /// The newest version this page knows of, once a save or a conflict has
    /// replaced `loaded`.
    @State var latest: NoteSummary? = nil

    /// A save that lost to a newer version, waiting for the person to choose.
    @State var conflict: EditConflict? = nil

    /// Whether the last save succeeded, for the status message.
    @State var isSaved: Bool = false

    /// Why the last save or delete didn't reach the server, for the alert.
    @State var problem: String? = nil

    /// The version the next save is based on.
    @Derived let current: NoteSummary = latest ?? loaded

    /// Saves a validated draft on top of `current`. A validation failure
    /// from the server is passed back to the editor's form.
    func save(draft: NoteDraft) async throws ValidationError {
        isSaved = false
        problem = nil

        do {
            latest = try await notes.update(note: current, draft: draft)
            conflict = nil
            isSaved = true
            await offline.discard(note: id)
        } catch .conflict(let newer) {
            conflict = EditConflict(theirs: newer, mine: draft)
        } catch .notFound {
            Router.navigate(to: .notes)
        } catch .rpcFailure(.invalid(let failure)) {
            throw failure
        } catch .rpcFailure(.rejected) {
            Router.navigate(to: .signIn.returnTo(.editNote(id: id)))
        } catch .rpcFailure(.offline), .rpcFailure(.timeout) {
            // Kept in this browser, so closing the tab doesn't lose it.
            if await offline.keep(note: id, draft: draft) {
                problem = "Couldn't reach K Notes, so this edit wasn't saved. This browser keeps it until you save it; your text is still here."
            } else {
                problem = "Couldn't reach K Notes, so this edit wasn't saved. Your text is still here; try again."
            }
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
        problem = nil

        do {
            try await notes.delete(note: current)

            Router.navigate(to: .notes)
        } catch .rpcFailure(.rejected) {
            Router.navigate(to: .signIn.returnTo(.editNote(id: id)))
        } catch .rpcFailure {
            problem = "Couldn't reach K Notes, so the note wasn't deleted. Try again."
        }
    }

    // Not evaluated when `failed` renders, because `current` doesn't exist
    // then. The `.notFound` arm sets its own title; the others show the
    // layout's.
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

        UnsavedEditNotice(noteId: id, save: save)

        NoteEditor(draft: loaded.draft(), save: save)

        link(to: .privateNote(id: id)) { "Private text" }

        if isSaved {
            paragraph { "Saved." }
                .class(.status)
                .role(.status)
        }

        if let problem {
            paragraph { problem }
                .class(.problem)
                .role(.alert)
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
                    title("Note not found · K Notes")
                    status(.notFound)
                }

                paragraph { "This note doesn't exist, or it was deleted." }

                link(to: .notes) { "Back to notes" }
            }

            case .rpcFailure(.rejected) {
                paragraph { "Your session has ended." }

                link(to: .signIn.returnTo(.editNote(id: id))) { "Sign in again" }
            }

            case .rpcFailure {
                paragraph { "Couldn't reach K Notes to load this note." }

                button { "Try again" }
                    .on(click: reload)
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

        .status  { color: .muted; }
        .problem { color: .danger; }
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
