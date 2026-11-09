import board.BoardId
import board.BoardService
import board.CardDraft
import board.Column
import k.validation.Form

/// Adds a card at the bottom of a column: a `@Form` of `CardDraft` with
/// the same rules as the editor, so a blank title is reported under the
/// field and adds nothing.
component NewCardForm {
    /// The board.
    @Prop let board: BoardId
    /// The column the card goes to.
    @Prop let column: Column

    /// The form closed: the card was added, or the user cancelled.
    @Emits let closed: Event<Void>

    @Inject let boards: BoardService

    @Form var input: CardDraft = .empty

    func add() {
        boards.addCard(board: board, column: column.id, draft: input.value)
        closed.emit()
    }

    view {
        form {
            textField(value: bind input.title)
                .label("New card in {column.title}")
                .placeholder("What needs doing?")
                .autofocus(true)

            box {
                button { "Add card" }
                    .type(.submit)

                button { "Cancel" }
                    .type(.button)
                    .on(click: { closed.emit() })
            }
            .class(.buttons)
        }
        .label("New card in {column.title}")
        .class(.form)
        .on(submit: input.handle(add))
    }

    style {
        .form { display: grid; gap: .sm; }
        .buttons { display: flex; gap: .xs; }
    }
}
