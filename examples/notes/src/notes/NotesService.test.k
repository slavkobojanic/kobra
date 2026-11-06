import accounts.AccountId
import k.auth.SessionManager
import k.crypto.Uuid
import k.rpc.RpcOnlyError
import k.test.TestClock

// Integration tests: no fakes, so these run NotesService, NotesProtocol
// in-process, NotesApi, the repositories and a rolled-back SQLite database.
// The test acts as one browser tab; `sessions.signIn` switches accounts
// without creating them.
test NotesService {
    @Inject let notes: NotesService
    @Inject let sessions: SessionManager
    @Inject let clock: TestClock

    // Writing

    test "a new note gets a trimmed title, parsed tags and revision 1" {
        await sessions.signIn(userId: AccountId.ada)

        let note = try await notes.create(draft: draft(title: "  Groceries ", tags: "home, Errands, home"))

        expect note.title == "Groceries"
        expect note.tags == ["errands", "home"]
        expect not note.isPinned
        expect note.revision == 1
        expect note.updatedAt == clock.now
        expect notes.all == [note]
    }

    test "an edit bumps the revision and the edit time, and replaces the tags" {
        await sessions.signIn(userId: AccountId.ada)
        let original = try await notes.create(draft: draft(title: "Plan", tags: "work"))
        clock.advance(by: 5.minutes)

        let edited = try await notes.update(note: original, draft: draft(title: "Plan B", tags: "ideas"))

        expect edited.revision == 2
        expect edited.updatedAt == clock.now
        expect edited.tags == ["ideas"]
    }

    test "saving over a newer version is a conflict, and changes nothing" {
        await sessions.signIn(userId: AccountId.ada)
        let original = try await notes.create(draft: draft(title: "Plan"))
        let otherTab = try await notes.update(note: original, draft: draft(title: "Plan from the other tab"))

        expect throws SaveError.conflict {
            try await notes.update(note: original, draft: draft(title: "Plan from this tab"))
        }

        let stored = try await notes.get(id: original.id)

        expect stored == otherTab
    }

    test "pinning isn't an edit, so it never causes a conflict" {
        await sessions.signIn(userId: AccountId.ada)
        let original = try await notes.create(draft: draft(title: "Plan"))
        clock.advance(by: 1.hour)

        try await notes.togglePin(note: original)
        let pinned = try await notes.get(id: original.id)
        let edited = try await notes.update(note: original, draft: draft(title: "Plan B"))

        expect pinned.isPinned
        expect pinned.revision == original.revision
        expect pinned.updatedAt == original.updatedAt
        expect edited.isPinned
    }

    test "deleting removes the note, and deleting it again does nothing" {
        await sessions.signIn(userId: AccountId.ada)
        let note = try await notes.create(draft: draft(title: "Groceries", tags: "home"))

        try await notes.delete(note: note)
        try await notes.delete(note: note)

        expect throws LookupError.notFound {
            try await notes.get(id: note.id)
        }

        expect notes.all.isEmpty
    }

    // Ordering

    test "pinned notes come first, then the most recently edited" {
        await sessions.signIn(userId: AccountId.ada)
        let groceries = try await notes.create(draft: draft(title: "Groceries"))
        clock.advance(by: 1.minute)
        try await notes.create(draft: draft(title: "Ideas"))
        clock.advance(by: 1.minute)
        try await notes.create(draft: draft(title: "Travel"))

        try await notes.togglePin(note: groceries)

        expect titles(notes: notes.all) == ["Groceries", "Travel", "Ideas"]
    }

    // Isolation and guards

    test "another account's note behaves exactly like a missing one" {
        await sessions.signIn(userId: AccountId.ada)
        let adas = try await notes.create(draft: draft(title: "Private"))

        await sessions.signIn(userId: AccountId.ben)

        expect throws LookupError.notFound {
            try await notes.get(id: adas.id)
        }

        expect throws SaveError.notFound {
            try await notes.update(note: adas, draft: draft(title: "Mine now"))
        }

        expect throws LookupError.notFound {
            try await notes.togglePin(note: adas)
        }

        try await notes.delete(note: adas)
        await sessions.signIn(userId: AccountId.ada)
        let stored = try await notes.get(id: adas.id)

        expect stored == adas
    }

    test "a visitor who isn't signed in is refused" {
        expect throws RpcOnlyError.rpcFailure(.rejected(.unauthenticated)) {
            try await notes.create(draft: draft(title: "Anonymous"))
        }

        expect throws LookupError.rpcFailure(.rejected(.unauthenticated)) {
            try await notes.get(id: Uuid.random())
        }
    }

    // Applying streamed changes

    test "a snapshot replaces the list, sorts it and marks it live" {
        let older = summary(title: "Older", updatedAt: clock.now)
        clock.advance(by: 1.minute)
        let newer = summary(title: "Newer", updatedAt: clock.now)

        notes.apply(change: .snapshot([older, newer]))

        expect notes.isLive
        expect titles(notes: notes.all) == ["Newer", "Older"]
    }

    test "a note saved in another tab replaces the old copy and moves into place" {
        let first = summary(title: "First", updatedAt: clock.now)
        clock.advance(by: 1.minute)
        let second = summary(title: "Second", updatedAt: clock.now)
        notes.apply(change: .snapshot([first, second]))

        notes.apply(change: .saved(first.copy(isPinned: true)))

        expect titles(notes: notes.all) == ["First", "Second"]
        expect notes.all[0].isPinned
    }

    test "applying the same change twice is the same as applying it once" {
        let note = summary(title: "Groceries", updatedAt: clock.now)

        notes.apply(change: .saved(note))
        notes.apply(change: .saved(note))

        expect notes.all == [note]

        notes.apply(change: .deleted(note.id))
        notes.apply(change: .deleted(note.id))

        expect notes.all.isEmpty
    }
}

/// A draft with only the fields a test cares about. Shared by this module's
/// tests.
func draft(title: String, body: String = "", tags: String = "") -> NoteDraft {
    return NoteDraft(title: title, body: body, tags: tags)
}

/// A note as the server would send it. Shared by this module's tests, which
/// pass `clock.now` as `updatedAt`: a plain function can't read the clock.
func summary(title: String, body: String = "", tags: List<String> = [], isPinned: Bool = false, updatedAt: Date) -> NoteSummary {
    return NoteSummary(id: Uuid.random(), title: title, body: body, tags: tags, isPinned: isPinned, revision: 1, updatedAt: updatedAt)
}

/// The titles of `notes`, in order.
func titles(notes: List<NoteSummary>) -> List<String> {
    return notes.map { note => note.title }
}
