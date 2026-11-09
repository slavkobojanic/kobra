import accounts.AccountId
import k.auth.SessionManager
import k.crypto.Uuid
import k.http.HttpRequest
import k.http.HttpResponse
import k.server.PublicUrls
import k.test.TestClient
import k.test.render

test BookDetails {
    @Inject let records: BookRepository
    @Inject let shelf: BookService
    @Inject let sessions: SessionManager

    /// The account the tests sign in as.
    let reader = AccountId(value: Uuid("0b6f2c1e-4d1a-4f8e-9a3b-2c5d6e7f8a9b"))

    test "the page shows the book" {
        await sessions.signIn(userId: reader)
        let book = try await shelf.add(draft: BookDraft(title: "Piranesi", author: "Susanna Clarke"))

        let screen = render(BookDetails(id: book.id))
        await screen.settle()

        expect screen.heading(named: "Piranesi").isVisible
        expect screen.text("by Susanna Clarke").isVisible
        expect screen.link(named: "All books").isVisible
    }

    test "notes and a rating are saved on the server" {
        await sessions.signIn(userId: reader)
        let book = try await shelf.add(draft: BookDraft(title: "Piranesi", author: "Susanna Clarke"))
        let screen = render(BookDetails(id: book.id))
        await screen.settle()

        screen.dropdown(labeled: "Rating").fill("4")
        screen.textArea(labeled: "Notes").fill("The house is the world.")
        screen.button(named: "Save").click()
        await screen.settle()

        expect screen.status().text == "Saved."

        let saved = await records.find(id: book.id, ownerId: reader)

        expect saved?.rating == 4
        expect saved?.notes == "The house is the world."
    }

    test "a book that doesn't exist isn't found" {
        await sessions.signIn(userId: reader)
        let screen = render(BookDetails(id: Uuid.random()))
        await screen.settle()

        expect screen.text("This book isn't on your shelf.").isVisible
    }
}

test BookDetailsOverHttp {
    @Inject let client: TestClient
    @Inject let urls: PublicUrls
    @Inject let sessions: SessionManager
    @Inject let shelf: BookService

    /// Asks the server for the page of the book with this id, as a browser
    /// opening the URL would.
    func open(id: Uuid) async -> HttpResponse {
        return await client.send(request: HttpRequest(method: .get, url: urls.absolute(.bookDetails(id: id))))
    }

    test "someone signed out is sent to sign in, and back afterwards" {
        let id = Uuid.random()

        let response = await open(id: id)

        expect response.status == .seeOther
        expect response.headers["location"] == "{Page.signIn.returnTo(.bookDetails(id: id)).url}"
    }

    test "the page arrives with the book's title" {
        await sessions.signIn(userId: AccountId(value: Uuid.random()))
        let book = try await shelf.add(draft: BookDraft(title: "Piranesi", author: "Susanna Clarke"))

        let response = await open(id: book.id)

        expect response.status == .ok
        expect response.body.contains("<title>Piranesi · Shelf</title>")
    }

    test "a book that doesn't exist answers 404" {
        await sessions.signIn(userId: AccountId(value: Uuid.random()))

        let response = await open(id: Uuid.random())

        expect response.status == .notFound
        expect response.body.contains("<title>Book not found · Shelf</title>")
    }
}
