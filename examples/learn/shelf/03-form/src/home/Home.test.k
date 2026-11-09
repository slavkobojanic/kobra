import k.test.render

test Home {
    test "the start page shows the app's name" {
        let screen = render(Home())

        expect screen.heading(named: "Shelf").isVisible
    }

    test "a book added with the form joins the list" {
        let screen = render(Home())

        screen.textField(labeled: "Title").fill("Piranesi")
        screen.textField(labeled: "Author").fill("Susanna Clarke")
        screen.button(named: "Add book").click()

        expect screen.item(containing: "Piranesi").isVisible
    }

    test "a book needs a title" {
        let screen = render(Home())

        screen.textField(labeled: "Author").fill("Susanna Clarke")
        screen.button(named: "Add book").click()

        expect screen.text("Enter the book's title").isVisible
        expect screen.items().count == 3
    }
}
