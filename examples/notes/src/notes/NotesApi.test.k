import accounts.AccountId
import k.auth.SessionManager
import k.crypto.Uuid
import k.test.TestClock

// The protocol called in-process, as another tab would call it: these tests
// read the `watch()` stream directly instead of through NotesService.
// `NoteChanges` is the server's own instance, injected from the test's
// server container.
test NotesApi {
    @Inject let api: NotesProtocol
    @Inject let sessions: SessionManager
    @Inject let changes: NoteChanges
    @Inject let clock: TestClock

    test "watch opens with a snapshot of the account's notes, pinned first" {
        await sessions.signIn(userId: AccountId.ada)
        let groceries = try await api.create(draft: draft(title: "Groceries"))
        clock.advance(by: 1.minute)
        let ideas = try await api.create(draft: draft(title: "Ideas"))
        let pinned = try await api.setPinned(id: groceries.id, isPinned: true)

        let stream = try await api.watch()

        expect await stream.next() == .snapshot([pinned, ideas])
    }

    test "watch delivers every later change to the account, in order" {
        await sessions.signIn(userId: AccountId.ada)
        let stream = try await api.watch()

        let created = try await api.create(draft: draft(title: "Groceries"))
        let edited = try await api.update(id: created.id, draft: draft(title: "Groceries for Sunday"), revision: created.revision)
        try await api.delete(id: created.id)

        expect await stream.next() == .snapshot([])
        expect await stream.next() == .saved(created)
        expect await stream.next() == .saved(edited)
        expect await stream.next() == .deleted(created.id)
    }

    test "watch never delivers another account's notes or changes" {
        await sessions.signIn(userId: AccountId.ben)
        try await api.create(draft: draft(title: "Ben's first note"))

        await sessions.signIn(userId: AccountId.ada)
        let adasChanges = try await api.watch()
        let bens = summary(title: "Ben's note")
        let adas = summary(title: "Ada's note")

        // What NotesApi publishes when Ben saves on another device. The test
        // has one session, so signing in as Ben here would end Ada's stream.
        changes.publish(ownerId: AccountId.ben, change: .saved(bens))
        changes.publish(ownerId: AccountId.ada, change: .saved(adas))

        expect await adasChanges.next() == .snapshot([])
        expect await adasChanges.next() == .saved(adas)
    }

    test "signing out ends the stream" {
        await sessions.signIn(userId: AccountId.ada)
        let stream = try await api.watch()

        expect await stream.next() == .snapshot([])

        await sessions.signOut()

        expect await stream.next() == nil
    }

    test "a refused save and a repeated delete announce nothing" {
        await sessions.signIn(userId: AccountId.ada)
        let original = try await api.create(draft: draft(title: "Plan"))
        let stream = try await api.watch()

        expect throws SaveError.conflict {
            try await api.update(id: original.id, draft: draft(title: "Stale"), revision: original.revision + 1)
        }

        try await api.delete(id: Uuid.random())
        let next = try await api.create(draft: draft(title: "Next"))

        expect await stream.next() == .snapshot([original])
        expect await stream.next() == .saved(next)
    }

    test "every rpc requires a signed-in session" {
        expect throws AccessError.rpcFailure(.rejected(.unauthenticated)) {
            try await api.watch()
        }

        expect throws SaveError.rpcFailure(.rejected(.unauthenticated)) {
            try await api.update(id: Uuid.random(), draft: draft(title: "Plan"), revision: 1)
        }

        expect throws AccessError.rpcFailure(.rejected(.unauthenticated)) {
            try await api.delete(id: Uuid.random())
        }
    }
}
