import board.BoardId
import board.BoardService
import board.CardId
import board.Column
import board.cardDrag
import k.drag.ReorderableList
import k.ui.ElementRef
import k.ui.VisuallyHidden

/// One column: its heading, its cards in a reorderable list, and the form
/// that adds a card.
///
/// Every column's list has the kind `cardDrag`, so cards move between
/// columns. The list moves nothing itself: its `moved` event goes to
/// `BoardService.moveCard`, and the board's new state redraws both
/// columns. The list's name is the column's title, so a screen reader and
/// the drag announcements say "Position 2 of 3 in Doing".
component ColumnView {
    /// The board the column is on.
    @Prop let board: BoardId
    /// The column.
    @Prop let column: Column
    /// Whether this is the board's last column, the one that counts as done.
    @Prop let isDone: Bool
    /// The card being edited, wherever it is.
    @Prop let editing: CardId?
    /// The card whose editor closed last, wherever it is: its Edit button
    /// takes the focus back.
    @Prop let lastEdited: CardId?
    /// Whether this column's new card form is open.
    @Prop let isAdding: Bool

    /// The user wants to edit a card.
    @Emits let edit: Event<CardId>
    /// The user wants to add a card here.
    @Emits let add: Event<Void>
    /// The editor or the new card form was closed.
    @Emits let closed: Event<Void>

    @Inject let boards: BoardService

    /// "Add a card", which takes the focus back when the new card form
    /// closes (`docs/design/53-element-references.md`).
    let addButton = ElementRef()

    @Derived let countLabel: String = if column.cards.count == 1 {
        "1 card"
    } else {
        "{column.cards.count} cards"
    }

    view {
        section {
            header {
                heading(level: 2) { column.title }

                text { countLabel }
                    .class(.count)
            }
            .class(.header)

            ReorderableList(kind: cardDrag, id: column.id.text, items: column.cards, key: { card => card.id.text }, label: column.title)
                .row { card =>
                    if editing == card.id {
                        CardEditor(board: board, card: card)
                            .on(closed: { closed.emit() })
                    } else {
                        CardView(card: card, isDone: isDone, takesFocus: lastEdited == card.id)
                            .on(edit: { edit.emit(card.id) })
                    }
                }
                .itemLabel { card => card.title }
                .on(moved: { move => boards.moveCard(board: board, move: move) })

            if isAdding {
                NewCardForm(board: board, column: column)
                    .on(closed: {
                        closed.emit()
                        addButton.focus()
                    })
            } else {
                button {
                    "Add a card"

                    VisuallyHidden { " to {column.title}" }
                }
                .type(.button)
                .class(.add)
                .ref(addButton)
                .on(click: { add.emit() })
            }
        }
        .label(column.title)
        .class([.column, .done when isDone])
    }

    style {
        .column {
            display: grid;
            gap: .sm;
            padding: .md;
            borderRadius: .md;
            backgroundColor: .column;
        }

        .done { border: 2px solid .chartDone; }

        .header {
            display: flex;
            alignItems: baseline;
            justifyContent: spaceBetween;
        }

        .count { color: .muted; fontSize: .sm; }

        .add {
            justifySelf: start;
            padding: .xs .sm;
            border: 1px dashed .border;
            borderRadius: .sm;
            backgroundColor: transparent;
            color: .text;
        }
    }
}
