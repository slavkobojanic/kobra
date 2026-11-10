import board.BoardId
import board.BoardService
import board.Card
import board.CardDraft
import board.Priority
import k.device.CopyButton
import k.server.PublicUrls
import k.validation.Form

/// Edits a card in place: its title and notes through a `@Form` of
/// `CardDraft`, validated as the user types and again on Save, and its
/// priority.
///
/// `form.handle(save)` runs `save` only for a valid draft, so a blank or
/// too long title is shown under its field and nothing is saved.
///
/// "Copy link" (`CopyButton`, `docs/design/45-web-platform.md`) copies the
/// card's link: the board page with this card's editor open. The boards
/// are in this browser's storage, so the link opens the card here, in
/// another tab or after a restart.
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

    @Inject let urls: PublicUrls

    /// The card's link, absolute: in the browser `PublicUrls` builds it
    /// against the page's origin, from the typed page as `link(to:)` does.
    @Derived let link: String = "{urls.absolute(.boardPage(id: board.value).card("{card.id.value}"))}"

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

                CopyButton(text: link, label: "Copy link to {card.title}")

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
