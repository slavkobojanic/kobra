import board.BoardId
import board.BoardService
import board.Card
import board.CardDraft
import board.Priority
import k.validation.Form

/// Edits a card in place: its title and notes through a `@Form` of
/// `CardDraft`, validated as the user types and again on Save, and its
/// priority.
///
/// `form.handle(save)` runs `save` only for a valid draft, so a blank or
/// too long title is shown under its field and nothing is saved.
component CardEditor {
    /// The board the card is on.
    @Prop let board: BoardId
    /// The card, as it is now.
    @Prop let card: Card

    /// The editor closed: saved or cancelled.
    @Emits let closed: Event<Void>

    @Inject let boards: BoardService

    @Form var input: CardDraft = .empty(title: card.title, notes: card.notes)

    @State var priority: Priority = card.priority

    func save() {
        boards.updateCard(board: board, card: card.id, draft: input.value, priority: priority)
        closed.emit()
    }

    view {
        form {
            textField(value: bind input.title)
                .label("Title of {card.title}")
                .autofocus(true)

            textArea(value: bind input.notes)
                .label("Notes of {card.title}")
                .rows(4)

            box {
                text { "Priority" }

                button { "Low" }
                    .type(.button)
                    .pressed(priority == .low)
                    .on(click: { priority = .low })

                button { "Normal" }
                    .type(.button)
                    .pressed(priority == .normal)
                    .on(click: { priority = .normal })

                button { "High" }
                    .type(.button)
                    .pressed(priority == .high)
                    .on(click: { priority = .high })
            }
            .class(.priorities)

            box {
                button { "Save" }
                    .type(.submit)

                button { "Cancel" }
                    .type(.button)
                    .on(click: { closed.emit() })

                button { "Delete card" }
                    .type(.button)
                    .class(.delete)
                    .on(click: {
                        boards.removeCard(board: board, card: card.id)
                        closed.emit()
                    })
            }
            .class(.buttons)
        }
        .label("Edit {card.title}")
        .class(.editor)
        .on(submit: input.handle(save))
    }

    style {
        .editor {
            display: grid;
            gap: .sm;
            padding: .sm;
            borderRadius: .sm;
            backgroundColor: .surface;
            border: 2px solid .primary;
        }

        .priorities, .buttons {
            display: flex;
            flexWrap: wrap;
            alignItems: center;
            gap: .xs;
        }

        .delete { marginLeft: auto; color: .danger; }
    }
}
