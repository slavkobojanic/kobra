import k.auth.Session
import k.crypto.Uuid
import k.test.TestClock
import k.time.Clock

// The protocol called in-process, as another tab would call it: these tests
// read the `watch()` stream directly instead of through NotesService.
test NotesApi {
    use TestClock for Clock

    @Inject let api: NotesProtocol
    @Inject let session: Session
    @Inject let clock: TestClock

    test "watch opens with a snapshot of the account's notes, pinned first" {
        await session.signIn(userId: "ada")
        let groceries = try await api.create(draft: draft(title: "Groceries"))
        clock.advance(by: 1.minute)
        let ideas = try await api.create(draft: draft(title: "Ideas"))
        let pinned = try await api.setPinned(id: groceries.id, isPinned: true)

        let changes = try await api.watch()

        expect await changes.next() == .snapshot([pinned, ideas])
    }

    test "watch delivers every later change to the account, in order" {
        await session.signIn(userId: "ada")
        let changes = try await api.watch()

        let created = try await api.create(draft: draft(title: "Groceries"))
        let edited = try await api.update(id: created.id, draft: draft(title: "Groceries for Sunday"), revision: created.revision)
        try await api.delete(id: created.id)

        expect await changes.next() == .snapshot([])
        expect await changes.next() == .saved(created)
        expect await changes.next() == .saved(edited)
        expect await changes.next() == .deleted(created.id)
    }

    test "watch never delivers another account's changes" {
        await session.signIn(userId: "ada")
        let adasChanges = try await api.watch()

        await session.signIn(userId: "ben")
        try await api.create(draft: draft(title: "Ben's note"))

        await session.signIn(userId: "ada")
        let adas = try await api.create(draft: draft(title: "Ada's note"))

        expect await adasChanges.next() == .snapshot([])
        expect await adasChanges.next() == .saved(adas)
    }

    test "a refused save and a repeated delete announce nothing" {
        await session.signIn(userId: "ada")
        let original = try await api.create(draft: draft(title: "Plan"))
        let changes = try await api.watch()

        expect throws SaveError.conflict {
            try await api.update(id: original.id, draft: draft(title: "Stale"), revision: original.revision + 1)
        }

        try await api.delete(id: Uuid.random())
        let next = try await api.create(draft: draft(title: "Next"))

        expect await changes.next() == .snapshot([original])
        expect await changes.next() == .saved(next)
    }

    test "every rpc requires a signed-in session" {
        expect throws AccessError.rejected(.unauthenticated) {
            try await api.watch()
        }

        expect throws SaveError.rejected(.unauthenticated) {
            try await api.update(id: Uuid.random(), draft: draft(title: "Plan"), revision: 1)
        }

        expect throws AccessError.rejected(.unauthenticated) {
            try await api.delete(id: Uuid.random())
        }
    }
}
