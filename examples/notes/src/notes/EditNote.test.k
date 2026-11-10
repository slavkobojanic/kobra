import accounts.AccountId
import k.auth.SessionManager
import k.crypto.Uuid
import k.test.TestConnectivity

test EditNote {
    @Inject let notes: NotesService
    @Inject let sessions: SessionManager
    @Inject let connectivity: TestConnectivity
    @Inject let offline: OfflineEdits

    test "saving stores the edit and confirms it" {
        await sessions.signIn(userId: AccountId.ada)
        let note = try await notes.create(draft: draft(title: "Plan", tags: "work"))
        let screen = render(EditNote(id: note.id))
        await screen.settle()

        expect screen.textField(labeled: "Title").value == "Plan"

        screen.textField(labeled: "Title").fill("Plan B")
        screen.button(named: "Save").click()
        await screen.settle()

        let stored = try await notes.get(id: note.id)

        expect screen.status().text == "Saved."
        expect stored.title == "Plan B"
        expect stored.tags == ["work"]
    }

    test "offline, the edit is kept in this browser and offered again until it is saved" {
        await sessions.signIn(userId: AccountId.ada)
        let note = try await notes.create(draft: draft(title: "Plan"))
        let screen = render(EditNote(id: note.id))
        await screen.settle()

        connectivity.goOffline()
        screen.textField(labeled: "Title").fill("Plan B")
        screen.button(named: "Save").click()
        await screen.settle()

        expect screen.alert().text == "Couldn't reach K Notes, so this edit wasn't saved. This browser keeps it until you save it; your text is still here."
        expect (await offline.edit(of: note.id))?.title == "Plan B"

        screen.unmount()
        connectivity.goOnline()
        let again = render(EditNote(id: note.id))
        await again.settle()

        expect again.text(containing: "“Plan B”").isVisible

        again.button(named: "Save that edit").click()
        await again.settle()

        expect (try await notes.get(id: note.id)).title == "Plan B"
        expect await offline.edit(of: note.id) == nil
        expect not again.button(named: "Save that edit").exists
    }

    test "a kept edit can be discarded" {
        await sessions.signIn(userId: AccountId.ada)
        let note = try await notes.create(draft: draft(title: "Plan"))
        await offline.keep(note: note.id, draft: draft(title: "Plan B"))
        let screen = render(EditNote(id: note.id))
        await screen.settle()

        screen.button(named: "Discard it").click()
        await screen.settle()

        expect await offline.edit(of: note.id) == nil
        expect (try await notes.get(id: note.id)).title == "Plan"
    }

    test "saving twice from the same tab is not a conflict" {
        await sessions.signIn(userId: AccountId.ada)
        let note = try await notes.create(draft: draft(title: "Plan"))
        let screen = render(EditNote(id: note.id))
        await screen.settle()

        screen.textField(labeled: "Title").fill("Plan B")
        screen.button(named: "Save").click()
        await screen.settle()
        screen.textField(labeled: "Title").fill("Plan C")
        screen.button(named: "Save").click()
        await screen.settle()

        let stored = try await notes.get(id: note.id)

        expect not screen.alert().exists
        expect stored.title == "Plan C"
        expect stored.revision == 3
    }

    test "an edit saved in another tab turns this save into a choice, and nothing is lost" {
        await sessions.signIn(userId: AccountId.ada)
        let note = try await notes.create(draft: draft(title: "Plan"))
        let screen = render(EditNote(id: note.id))
        await screen.settle()
        try await notes.update(note: note, draft: draft(title: "Plan from the other tab"))

        screen.textField(labeled: "Title").fill("Plan from this tab")
        screen.button(named: "Save").click()
        await screen.settle()

        let beforeChoosing = try await notes.get(id: note.id)

        expect screen.alert().isVisible
        expect screen.textField(labeled: "Title").value == "Plan from this tab"
        expect beforeChoosing.title == "Plan from the other tab"

        screen.button(named: "Keep my version").click()
        await screen.settle()

        let afterChoosing = try await notes.get(id: note.id)

        expect not screen.alert().exists
        expect afterChoosing.title == "Plan from this tab"
    }

    test "a note that doesn't exist shows a message instead of the editor" {
        await sessions.signIn(userId: AccountId.ada)
        let screen = render(EditNote(id: Uuid.random()))
        await screen.settle()

        expect screen.text("This note doesn't exist, or it was deleted.").isVisible
        expect screen.link(named: "Back to notes").isVisible
        expect not screen.textField(labeled: "Title").exists
    }

    test "deleting the note returns to the list" {
        await sessions.signIn(userId: AccountId.ada)
        let note = try await notes.create(draft: draft(title: "Plan"))
        let screen = render(EditNote(id: note.id))
        await screen.settle()

        screen.button(named: "Delete note").click()
        await screen.settle()

        expect screen.currentPage == .notes

        expect throws LookupError.notFound {
            try await notes.get(id: note.id)
        }
    }
}
