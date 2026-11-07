import board.Board
import board.BoardFile
import board.BoardService
import k.device.File
import k.encoding.Json
import k.test.MemoryStorage
import k.test.TestFiles
import k.ui.Theme

test Home {
    @Inject let service: BoardService
    @Inject let files: TestFiles
    @Inject let storage: MemoryStorage
    @Inject let theme: Theme

    test "with no boards, the page invites creating one" {
        let screen = render(Home())

        expect screen.heading(named: "Kanban").level == 1
        expect screen.text("No boards yet. Create one below, or import a board you exported.").isVisible
        expect screen.title == "Kanban"
    }

    test "creating a board opens it" {
        let screen = render(Home())
        let field = screen.textField(labeled: "New board")

        field.fill("Launch")
        field.press(.enter)

        expect service.boards.count == 1
        expect screen.currentPage == .boardPage(id: service.boards[0].id.value)
    }

    test "a blank name is refused, next to the field" {
        let screen = render(Home())

        screen.textField(labeled: "New board").fill("  ")
        screen.button(named: "Create board").click()

        expect screen.alert().text == "Give it a name."
        expect service.boards.isEmpty
    }

    test "boards are listed with their progress and link to themselves" {
        try Board.seed(in: service, cards: ["A", "B"])
        let screen = render(Home())

        expect screen.list(named: "Boards").items().count == 1
        expect screen.link(named: "Seed").exists
        expect screen.text("0 of 2 done").isVisible
    }

    test "importing a board file opens the imported board" {
        files.choose(File.fromText(name: "board.json", type: .json, text: Json.encode(BoardFile(format: 1, board: Board.sample))))
        let screen = render(Home())

        screen.button(named: "Import board…").click()
        await screen.settle()

        expect service.boards.count == 1
        expect screen.currentPage == .boardPage(id: Board.sample.id.value)
    }

    test "a file that isn't a board is reported and can be dismissed" {
        files.choose(File.fromText(name: "list.txt", type: .text, text: "milk, eggs"))
        let screen = render(Home())

        screen.button(named: "Import board…").click()
        await screen.settle()

        expect screen.status().text.startsWith("That file isn't a Kanban board")

        screen.status().button(named: "Dismiss").click()

        expect not screen.status().exists
    }

    test "the theme buttons switch the look and say which is on" {
        let screen = render(Home())

        screen.button(named: "Dark").click()

        expect theme.current == .dark
        expect screen.button(named: "Dark").isPressed
        expect not screen.button(named: "Light").isPressed

        screen.button(named: "High contrast").click()

        expect theme.current == .contrast
    }

    test "boards saved earlier are there on the next visit" {
        try Board.seed(in: service, cards: ["A"])
        service.restore()
        let screen = render(Home())

        expect screen.link(named: "Seed").exists
    }
}
