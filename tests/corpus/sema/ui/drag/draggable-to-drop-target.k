// A `Draggable` carries a typed payload to a `DropTarget` of the same kind; `dropped` delivers the
// payload with its type, and `accepts` is a rule on it (41-drag-and-drop.md).
import k.drag.DragKind
import k.drag.Draggable
import k.drag.DropTarget

struct CardDrag {
    let cardId: Int
    let isLocked: Bool
}

let cardDrag: DragKind<CardDrag> = DragKind(name: "card")

component Trash {
    @State var removed: List<Int> = []

    func discard(card: CardDrag) {
        removed.append(card.cardId)
    }

    view {
        Draggable(kind: cardDrag, payload: CardDrag(cardId: 7, isLocked: false), label: "Card 7") {
            text { "Card 7" }
        }

        DropTarget(kind: cardDrag, label: "Trash") {
            text { "Drop here to delete" }
        }
        .accepts { card => not card.isLocked }
        .on(dropped: discard)

        DropTarget(kind: cardDrag, label: "Archive") {
            text { "Archive" }
        }
        .on(dropped: { card => removed.append(card.cardId) })
        //             ^^^^ type: corpus.CardDrag
    }
}
