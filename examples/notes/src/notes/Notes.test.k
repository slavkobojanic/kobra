import accounts.AccountId
import k.auth.SessionManager
import k.test.TestClock

test Notes {
    @Inject let notes: NotesService
    @Inject let api: NotesProtocol
    @Inject let sessions: SessionManager
    @Inject let clock: TestClock

    test "the list shows the account's notes and filters as you type" {
        await sessions.signIn(userId: AccountId.ada)
        try await notes.create(draft: draft(title: "Groceries", body: "Milk and eggs", tags: "home"))
        try await notes.create(draft: draft(title: "Ideas", tags: "work"))
        let screen = render(Notes())
        await screen.settle()

        expect screen.link(named: "Groceries").isVisible
        expect screen.link(named: "Ideas").isVisible

        screen.textField(labeled: "Filter").fill("WORK")

        expect screen.link(named: "Ideas").isVisible
        expect not screen.link(named: "Groceries").exists
    }

    test "a filter with no matches says so" {
        await sessions.signIn(userId: AccountId.ada)
        try await notes.create(draft: draft(title: "Groceries"))
        let screen = render(Notes())
        await screen.settle()

        screen.textField(labeled: "Filter").fill("holiday")

        expect screen.status().text == "No notes match “holiday”."
    }

    test "a note saved in another tab appears without a reload" {
        await sessions.signIn(userId: AccountId.ada)
        let screen = render(Notes())
        await screen.settle()

        expect screen.text("No notes yet. Write your first one.").isVisible

        // Calling the protocol directly skips this tab's NotesService, as a
        // second tab would, so only the stream can update this page.
        try await api.create(draft: draft(title: "From the other tab"))
        await screen.settle()

        expect screen.link(named: "From the other tab").isVisible
    }

    test "pinning from a card moves the note to the top" {
        await sessions.signIn(userId: AccountId.ada)
        try await notes.create(draft: draft(title: "Groceries"))
        clock.advance(by: 1.minute)
        try await notes.create(draft: draft(title: "Ideas"))
        let screen = render(Notes())
        await screen.settle()

        screen.button(named: "Pin Groceries").click()
        await screen.settle()

        expect screen.headings().map { heading => heading.text } == ["Notes", "Groceries", "Ideas"]
        expect screen.button(named: "Pin Groceries").isPressed
    }

    test "a visitor who isn't signed in is sent to sign in, and back afterwards" {
        let screen = render(Notes())
        await screen.settle()

        expect screen.currentPage == .signIn.returnTo(.notes)
    }
}
