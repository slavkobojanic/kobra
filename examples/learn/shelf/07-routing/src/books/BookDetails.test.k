import k.crypto.Uuid
import k.test.render

test BookDetails {
    @Inject let records: BookRepository
    @Inject let shelf: BookService

    test "the page shows the book" {
        let book = try await shelf.add(draft: BookDraft(title: "Piranesi", author: "Susanna Clarke"))

        let screen = render(BookDetails(id: book.id))
        await screen.settle()

        expect screen.heading(named: "Piranesi").isVisible
        expect screen.text("by Susanna Clarke").isVisible
        expect screen.link(named: "All books").isVisible
    }

    test "notes and a rating are saved on the server" {
        let book = try await shelf.add(draft: BookDraft(title: "Piranesi", author: "Susanna Clarke"))
        let screen = render(BookDetails(id: book.id))
        await screen.settle()

        screen.dropdown(labeled: "Rating").fill("4")
        screen.textArea(labeled: "Notes").fill("The house is the world.")
        screen.button(named: "Save").click()
        await screen.settle()

        expect screen.status().text == "Saved."

        let saved = await records.find(id: book.id)

        expect saved?.rating == 4
        expect saved?.notes == "The house is the world."
    }

    test "a book that doesn't exist isn't found" {
        let screen = render(BookDetails(id: Uuid.random()))
        await screen.settle()

        expect screen.text("This book isn't on your shelf.").isVisible
    }
}
