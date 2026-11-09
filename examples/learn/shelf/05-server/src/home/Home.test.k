import books.BookRepository
import k.test.render

test Home {
    @Inject let records: BookRepository

    test "the start page shows the app's name" {
        let screen = render(Home())

        expect screen.heading(named: "Shelf").isVisible
    }

    test "a book added with the form joins the list" {
        let screen = render(Home())

        screen.textField(labeled: "Title").fill("Piranesi")
        screen.textField(labeled: "Author").fill("Susanna Clarke")
        screen.button(named: "Add book").click()
        await screen.settle()

        expect screen.item(containing: "Piranesi").isVisible
        expect screen.textField(labeled: "Title").value == ""
    }

    test "a book added with the form is saved on the server" {
        let screen = render(Home())

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

        screen.textField(labeled: "Author").fill("Susanna Clarke")
        screen.button(named: "Add book").click()
        await screen.settle()

        expect screen.text("Enter the book's title").isVisible
        expect screen.items().count == 0
    }
}
