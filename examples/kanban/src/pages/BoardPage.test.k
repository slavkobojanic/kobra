import board.Board
import board.BoardId
import board.BoardService
import board.TitleError
import k.crypto.Uuid
import k.test.TestFiles

test BoardPage {
    @Inject let service: BoardService
    @Inject let files: TestFiles

    /// A board named "Seed" with cards A and B in To do.
    func seeded() throws TitleError -> BoardId {
        return try Board.seed(in: service, cards: ["A", "B"])
    }

    test "the columns are lists named after them, holding their cards" {
        let id = try seeded()
        let screen = render(BoardPage(id: id.value))

        expect screen.heading(named: "Seed").level == 1
        expect screen.title == "Seed · Kanban"
        expect screen.list(named: "To do").items().count == 2
        expect screen.list(named: "Doing").items().count == 0
        expect screen.list(named: "Done").exists
        expect screen.text("2 cards").isVisible
    }

    test "Space, ArrowRight and Space move a card to the next column" {
        let id = try seeded()
        let screen = render(BoardPage(id: id.value))
        let card = screen.list(named: "To do").item(containing: "A")

        card.press(.space)
        card.press(.arrowRight)
        card.press(.space)

        expect screen.list(named: "To do").items().count == 1
        expect screen.list(named: "Doing").items().count == 1
        expect screen.list(named: "Doing").item(containing: "A").exists
        expect (service.board(id: id) ?? Board.sample).columns[1].cards[0].title == "A"
    }

    test "ArrowDown reorders a card within its column, Escape cancels" {
        let id = try seeded()
        let screen = render(BoardPage(id: id.value))
        let card = screen.list(named: "To do").item(containing: "A")

        card.press(.space)
        card.press(.arrowDown)
        card.press(.enter)

        expect screen.list(named: "To do").items()[0].text.startsWith("B")

        screen.list(named: "To do").item(containing: "B").press(.space)
        screen.list(named: "To do").item(containing: "B").press(.arrowRight)
        screen.list(named: "To do").item(containing: "B").press(.escape)

        expect screen.list(named: "To do").items().count == 2
    }

    test "a mouse drag onto the Done column finishes a card, and the progress follows" {
        let id = try seeded()
        let screen = render(BoardPage(id: id.value))

        screen.list(named: "To do").item(containing: "B").drag(to: screen.list(named: "Done"))

        expect screen.list(named: "Done").items().count == 1
        expect screen.text("Progress: 1 of 2 cards done").isVisible
        expect (service.board(id: id) ?? Board.sample).doneCount == 1
    }

    test "dropping a card on another card takes its place" {
        let id = try seeded()
        let screen = render(BoardPage(id: id.value))

        screen.list(named: "To do").item(containing: "B").drag(to: screen.list(named: "To do").item(containing: "A"))

        expect screen.list(named: "To do").items()[0].text.startsWith("B")
    }

    test "the editor validates the title, then saves title, notes and priority" {
        let id = try seeded()
        let screen = render(BoardPage(id: id.value))

        screen.button(named: "Edit A").click()
        screen.textField(labeled: "Title of A").fill("   ")
        screen.button(named: "Save").click()
        await screen.settle()

        expect screen.text("Give the card a title").isVisible
        expect (service.board(id: id) ?? Board.sample).columns[0].cards[0].title == "A"

        screen.textField(labeled: "Title of A").fill("A, revised")
        screen.textArea(labeled: "Notes of A").fill("Before Friday")
        screen.button(named: "High").click()
        screen.button(named: "Save").click()
        await screen.settle()

        let card = (service.board(id: id) ?? Board.sample).columns[0].cards[0]
        expect card.title == "A, revised"
        expect card.notes == "Before Friday"
        expect card.priority == .high
        expect screen.list(named: "To do").item(containing: "A, revised").exists
        expect not screen.textField(labeled: "Title of A").exists
    }

    test "a card can be deleted from the editor" {
        let id = try seeded()
        let screen = render(BoardPage(id: id.value))

        screen.button(named: "Edit B").click()
        screen.button(named: "Delete card").click()

        expect screen.list(named: "To do").items().count == 1
    }

    test "N opens the new card form of the first column; a blank title adds nothing" {
        let id = try seeded()
        let screen = render(BoardPage(id: id.value))

        screen.heading(named: "Seed").press(.character("n"))
        screen.textField(labeled: "New card in To do").fill("")
        screen.button(named: "Add card").click()
        await screen.settle()

        expect screen.text("Give the card a title").isVisible
        expect screen.list(named: "To do").items().count == 2

        screen.textField(labeled: "New card in To do").fill("C")
        screen.button(named: "Add card").click()
        await screen.settle()

        expect screen.list(named: "To do").items().count == 3
        expect not screen.textField(labeled: "New card in To do").exists
    }

    test "a shortcut key typed into a field is text, not a shortcut" {
        let id = try seeded()
        let screen = render(BoardPage(id: id.value))

        screen.button(named: "Add a card to Doing").click()
        screen.textField(labeled: "New card in Doing").press(.character("n"))

        expect not screen.textField(labeled: "New card in To do").exists
        expect screen.textField(labeled: "New card in Doing").exists
    }

    test "? shows the shortcuts, E exports, and the buttons do the same" {
        let id = try seeded()
        let screen = render(BoardPage(id: id.value))

        screen.heading(named: "Seed").press(.character("?"))

        expect screen.heading(named: "Keyboard shortcuts").exists

        screen.heading(named: "Seed").press(.character("e"))
        screen.button(named: "Export (E)").click()

        expect files.saved.count == 2
        expect files.saved[0].name == "seed.json"
    }

    test "a board this browser doesn't have is not found" {
        let screen = render(BoardPage(id: Uuid("2f0a2f4c-5a0b-4c3e-9d1e-7b6a5c4d3e2f")))

        expect screen.heading(named: "Board not found").exists
        expect screen.link(named: "All boards").exists
    }
}
