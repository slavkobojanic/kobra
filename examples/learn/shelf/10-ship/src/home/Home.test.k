import accounts.AccountId
import books.BookDraft
import books.BookRepository
import books.BookService
import k.auth.SessionManager
import k.crypto.Uuid
import k.test.render
import testing.OfflineBookService

test Home {
    @Inject let records: BookRepository
    @Inject let shelf: BookService
    @Inject let sessions: SessionManager

    /// The account the tests sign in as.
    let reader = AccountId(value: Uuid("0b6f2c1e-4d1a-4f8e-9a3b-2c5d6e7f8a9b"))

    test "the start page shows the app's name" {
        await sessions.signIn(userId: reader)
        let screen = render(Home())
        await screen.settle()

        expect screen.heading(named: "Shelf").isVisible
    }

    test "the page says it's loading until the books arrive" {
        await sessions.signIn(userId: reader)
        let screen = render(Home())

        expect screen.text("Loading your books…").isVisible

        await screen.settle()

        expect screen.text("No books yet. Add the first one below.").isVisible
    }

    test "the saved books are listed when the page opens" {
        await sessions.signIn(userId: reader)
        try await shelf.add(draft: BookDraft(title: "Piranesi", author: "Susanna Clarke"))
        try await shelf.add(draft: BookDraft(title: "Middlemarch", author: "George Eliot"))

        let screen = render(Home())
        await screen.settle()

        expect screen.items().count == 2
        expect screen.item(containing: "Piranesi").isVisible
        expect screen.link(named: "Piranesi").isVisible
    }

    test "a book added with the form joins the list" {
        await sessions.signIn(userId: reader)
        let screen = render(Home())
        await screen.settle()

        screen.textField(labeled: "Title").fill("Piranesi")
        screen.textField(labeled: "Author").fill("Susanna Clarke")
        screen.button(named: "Add book").click()
        await screen.settle()

        expect screen.item(containing: "Piranesi").isVisible
        expect screen.textField(labeled: "Title").value == ""
    }

    test "a book added with the form is saved on the server" {
        await sessions.signIn(userId: reader)
        let screen = render(Home())
        await screen.settle()

        screen.textField(labeled: "Title").fill("Piranesi")
        screen.textField(labeled: "Author").fill("Susanna Clarke")
        screen.button(named: "Add book").click()
        await screen.settle()

        let saved = await records.all(ownerId: reader)

        expect saved.count == 1
        expect saved[0].title == "Piranesi"
    }

    test "a book needs a title" {
        await sessions.signIn(userId: reader)
        let screen = render(Home())
        await screen.settle()

        screen.textField(labeled: "Author").fill("Susanna Clarke")
        screen.button(named: "Add book").click()
        await screen.settle()

        expect screen.text("Enter the book's title").isVisible
        expect screen.items().count == 0
    }

    test "each person sees only their own books" {
        await sessions.signIn(userId: reader)
        try await shelf.add(draft: BookDraft(title: "Piranesi", author: "Susanna Clarke"))

        await sessions.signIn(userId: AccountId(value: Uuid.random()))
        let screen = render(Home())
        await screen.settle()

        expect screen.text("No books yet. Add the first one below.").isVisible
    }
}

test HomeOffline {
    use OfflineBookService for BookService

    @Inject let shelf: OfflineBookService
    @Inject let sessions: SessionManager

    test "the page says when the server can't be reached" {
        await sessions.signIn(userId: AccountId(value: Uuid.random()))
        let screen = render(Home())
        await screen.settle()

        expect screen.text("Shelf can't reach the server. Check your connection.").isVisible
        expect screen.button(named: "Try again").isVisible
    }

    test "trying again loads the books once the server answers" {
        await sessions.signIn(userId: AccountId(value: Uuid.random()))
        let screen = render(Home())
        await screen.settle()

        shelf.reconnect()
        screen.button(named: "Try again").click()
        await screen.settle()

        expect screen.text("No books yet. Add the first one below.").isVisible
    }
}
