import accounts.AccountId
import k.auth.SessionManager
import k.test.TestConnectivity

test NewNote {
    @Inject let sessions: SessionManager
    @Inject let connectivity: TestConnectivity

    test "offline, creating a note says so and keeps the text" {
        await sessions.signIn(userId: AccountId.ada)
        let screen = render(NewNote())
        await screen.settle()

        connectivity.goOffline()
        screen.textField(labeled: "Title").fill("Groceries")
        screen.textArea(labeled: "Note").fill("Milk and eggs")
        screen.button(named: "Create note").click()
        await screen.settle()

        expect screen.alert().text == "Couldn't reach K Notes, so the note wasn't created. Your text is still here; try again."
        expect screen.textField(labeled: "Title").value == "Groceries"

        connectivity.goOnline()
        screen.button(named: "Create note").click()
        await screen.settle()

        expect screen.currentPage == .notes
    }
}
