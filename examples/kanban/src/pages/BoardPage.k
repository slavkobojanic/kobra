import board.Board
import board.BoardId
import board.BoardService
import board.BoardTransfer
import board.CardId
import board.ColumnId
import k.crypto.Uuid
import k.ui.KeyboardShortcut
import ui.Notice
import ui.ThemePicker

/// One board: its columns side by side, cards dragged between them with a
/// pointer, a finger or the keyboard, the card editor, the progress chart,
/// and export.
///
/// Keyboard shortcuts (`KeyboardShortcut`, `docs/design/43-keyboard-shortcuts.md`):
/// N adds a card to the first column, E exports the board, ? shows the
/// list of shortcuts and Escape closes the editor. Each one is also a
/// button on the page.
///
/// The URL is named (`/boards/<id>`), because exported links and
/// bookmarks depend on it.
public page BoardPage at "/boards/{id}" {
    /// The board's id.
    @Param let id: Uuid

    @Inject let boards: BoardService
    @Inject let transfer: BoardTransfer

    /// The card being edited, or `nil`.
    @State var editing: CardId? = nil

    /// The column whose new card form is open, or `nil`.
    @State var adding: ColumnId? = nil

    /// Whether the list of shortcuts is shown.
    @State var isShowingShortcuts: Bool = false

    @Derived let board: Board? = boards.board(id: BoardId(value: id))

    @Derived let documentTitle: String = if let board {
        "{board.title} · Kanban"
    } else {
        "Board not found · Kanban"
    }

    func startNewCard() {
        if let first = board?.columns.first {
            editing = nil
            adding = first.id
        }
    }

    func exportBoard() {
        transfer.export(board: BoardId(value: id))
    }

    func closeEditor() {
        editing = nil
        adding = nil
        isShowingShortcuts = false
    }

    metadata {
        title(documentTitle)
    }

    view {
        main {
            KeyboardShortcut(key: "n", label: "Add a card to the first column")
                .on(pressed: startNewCard)

            KeyboardShortcut(key: "e", label: "Export the board")
                .on(pressed: exportBoard)

            KeyboardShortcut(key: "?", label: "Show keyboard shortcuts")
                .on(pressed: { isShowingShortcuts = not isShowingShortcuts })

            KeyboardShortcut(key: "Escape", label: "Close the editor")
                .on(pressed: closeEditor)

            if let board {
                header {
                    link(to: .home) { "All boards" }

                    heading(level: 1) { board.title }

                    box {
                        button { "New card (N)" }
                            .type(.button)
                            .on(click: startNewCard)

                        button { "Export (E)" }
                            .type(.button)
                            .on(click: exportBoard)

                        button { "Shortcuts (?)" }
                            .type(.button)
                            .expanded(isShowingShortcuts)
                            .on(click: { isShowingShortcuts = not isShowingShortcuts })

                        ThemePicker()
                    }
                    .class(.actions)
                }
                .class(.header)

                if isShowingShortcuts {
                    ShortcutList()
                }

                if let problem = boards.problem {
                    Notice(message: problem.message)
                        .on(dismissed: boards.dismissProblem)
                }

                if let problem = transfer.problem {
                    Notice(message: problem.message)
                        .on(dismissed: transfer.dismissProblem)
                }

                box {
                    for column in board.columns key column.id {
                        ColumnView(board: board.id, column: column, isDone: column.id == board.columns.last?.id, editing: editing, isAdding: adding == column.id)
                            .on(edit: { card => editing = card })
                            .on(add: { adding = column.id })
                            .on(closed: closeEditor)
                    }
                }
                .class(.columns)

                ProgressChart(board: board)
            } else {
                heading(level: 1) { "Board not found" }

                paragraph { "This browser has no board with this address. It may have been deleted, or exported from another browser." }

                link(to: .home) { "All boards" }
            }
        }
        .class(.page)
    }

    style {
        .page {
            minHeight: 100vh;
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
            flexWrap: wrap;
            alignItems: center;
            gap: .md;
        }

        .actions {
            display: flex;
            flexWrap: wrap;
            gap: .sm;
            marginLeft: auto;
        }

        .columns {
            display: grid;
            gridTemplateColumns: repeat(autoFit, minmax(16rem, 1fr));
            gap: .md;
            alignItems: start;
        }
    }
}
