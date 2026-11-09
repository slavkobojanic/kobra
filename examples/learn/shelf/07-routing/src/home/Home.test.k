import books.BookDraft
import books.BookRepository
import books.BookService
import k.test.render

test Home {
    @Inject let records: BookRepository
    @Inject let shelf: BookService

    test "the start page shows the app's name" {
        let screen = render(Home())
        await screen.settle()

        expect screen.heading(named: "Shelf").isVisible
    }

    test "the page says it's loading until the books arrive" {
        let screen = render(Home())

        expect screen.text("Loading your books…").isVisible

        await screen.settle()

        expect screen.text("No books yet. Add the first one below.").isVisible
    }

    test "the saved books are listed when the page opens" {
        try await shelf.add(draft: BookDraft(title: "Piranesi", author: "Susanna Clarke"))
        try await shelf.add(draft: BookDraft(title: "Middlemarch", author: "George Eliot"))

        let screen = render(Home())
        await screen.settle()

        expect screen.items().count == 2
        expect screen.item(containing: "Piranesi").isVisible
        expect screen.link(named: "Piranesi").isVisible
    }

    test "a book added with the form joins the list" {
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
        let screen = render(Home())
        await screen.settle()

        screen.textField(labeled: "Title").fill("Piranesi")
        screen.textField(labeled: "Author").fill("Susanna Clarke")
        screen.button(named: "Add book").click()
        await screen.settle()

        let saved = await records.all()

        expect saved.count == 1
        expect saved[0].title == "Piranesi"
    }

    test "a book needs a title" {
        let screen = render(Home())
        await screen.settle()

        screen.textField(labeled: "Author").fill("Susanna Clarke")
        screen.button(named: "Add book").click()
        await screen.settle()

        expect screen.text("Enter the book's title").isVisible
        expect screen.items().count == 0
    }
}
