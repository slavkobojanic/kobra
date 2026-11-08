import board.BoardService
import board.BoardTransfer
import board.TitleError
import k.ui.Router
import ui.Notice
import ui.ThemePicker

/// The list of boards: open one, create one, or import one from a file.
///
/// Creating or importing a board opens it. The boards come from
/// `BoardService`, so this page holds only the new board's name and the
/// name's error.
public page Home at "/" {
    @Inject let boards: BoardService
    @Inject let transfer: BoardTransfer

    @State var newTitle: String = ""
    @State var error: TitleError? = nil

    func create() {
        do {
            let id = try boards.createBoard(title: newTitle)
            newTitle = ""
            error = nil
            Router.navigate(to: .boardPage(id: id.value))
        } catch let rejection {
            error = rejection
        }
    }

    // Called by the Import button's click, so the picker opens in the
    // user's action.
    func importFile() async {
        if let id = await transfer.importBoard() {
            Router.navigate(to: .boardPage(id: id.value))
        }
    }

    metadata {
        title("Kanban")
    }

    view {
        main {
            header {
                heading(level: 1) { "Kanban" }

                ThemePicker()
            }
            .class(.header)

            if let problem = boards.problem {
                Notice(message: problem.message)
                    .on(dismissed: boards.dismissProblem)
            }

            if let problem = transfer.problem {
                Notice(message: problem.message)
                    .on(dismissed: transfer.dismissProblem)
            }

            section {
                heading(level: 2) { "Boards" }

                if boards.boards.isEmpty {
                    paragraph { "No boards yet. Create one below, or import a board you exported." }
                        .class(.empty)
                } else {
                    list {
                        for board in boards.boards key board.id {
                            item {
                                link(to: .boardPage(id: board.id.value)) { board.title }

                                text { "{board.doneCount} of {board.cardCount} done" }
                                    .class(.count)
                            }
                            .class(.board)
                        }
                    }
                    .label("Boards")
                    .class(.boards)
                }
            }
            .label("Boards")

            form {
                textField(value: bind newTitle)
                    .label("New board")
                    .placeholder("Launch plan")

                button { "Create board" }
                    .type(.submit)

                button { "Import board…" }
                    .type(.button)
                    .on(click: importFile)

                if let error {
                    paragraph { error.message }
                        .role(.alert)
                        .class(.error)
                }
            }
            .label("New board")
            .class(.create)
            .on(submit: create)
        }
        .class(.page)
    }

    style {
        .page {
            minHeight: 100vh;
            maxWidth: 48rem;
            margin: 0 auto;
            padding: .lg .md;
            display: grid;
            gap: .lg;
            alignContent: start;
            backgroundColor: .background;
            color: .text;
            font: .body;
        }

        .header {
            display: flex;
            alignItems: center;
            justifyContent: spaceBetween;
            gap: .md;
            flexWrap: wrap;
        }

        .boards {
            margin: 0;
            padding: 0;
            listStyle: none;
            display: grid;
            gap: .sm;
        }

        .board {
            display: flex;
            justifyContent: spaceBetween;
            padding: .md;
            borderRadius: .md;
            backgroundColor: .surface;
            border: 1px solid .border;
        }

        .count { color: .muted; fontSize: .sm; }
        .empty { color: .muted; }

        .create {
            display: flex;
            flexWrap: wrap;
            gap: .sm;
            alignItems: end;
        }

        .error {
            flexBasis: 100%;
            margin: 0;
            color: .danger;
        }
    }
}
