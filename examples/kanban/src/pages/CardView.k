import board.Card
import k.ui.ElementRef
import k.ui.VisuallyHidden

/// A card as it sits in its column: title, priority, the start of its
/// notes, and an Edit button named after it ("Edit Write tests").
///
/// The whole card is the drag handle (its list item is), except the
/// button, which keeps its own click.
///
/// When the card's editor closes, the card is shown again and its Edit
/// button takes the focus back (`takesFocus`), so a keyboard user carries
/// on from the card instead of from the top of the page
/// (`docs/design/53-element-references.md`).
component CardView {
    /// The card.
    @Prop let card: Card
    /// Whether the card is in the done column.
    @Prop let isDone: Bool

    /// Whether the card's editor just closed: the Edit button takes the
    /// focus when the card is shown.
    @Prop let takesFocus: Bool = false

    /// The user wants to edit the card.
    @Emits let edit: Event<Void>

    let editButton = ElementRef()

    @Effect(on: [])
    func returnFocus() {
        if takesFocus {
            editButton.focus()
        }
    }

    view {
        article {
            heading(level: 3) { card.title }
                .class(.title)

            text { card.priority.label }
                .class([.priority, .high when card.priority == .high, .low when card.priority == .low])

            if card.notes.isNotEmpty {
                paragraph { card.notes.truncated(length: 120) }
                    .class(.notes)
            }

            button {
                "Edit"

                VisuallyHidden { " {card.title}" }
            }
            .type(.button)
            .class(.edit)
            .ref(editButton)
            .on(click: { edit.emit() })
        }
        .class([.card, .finished when isDone])
    }

    style {
        .card {
            display: grid;
            gap: .xs;
            padding: .sm .md;
            borderRadius: .sm;
            backgroundColor: .surface;
            border: 1px solid .border;
            cursor: grab;
        }

        .finished .title { textDecoration: lineThrough; }

        .title { margin: 0; fontSize: 1rem; }

        .priority { fontSize: .sm; color: .muted; }
        .high { color: .high; fontWeight: 600; }
        .low { color: .low; }

        .notes { margin: 0; color: .muted; fontSize: .sm; }

        .edit { justifySelf: end; }
    }
}
