// A drop handler receives the kind's payload type: a handler of another type is K2001 at the handler,
// so a drop can never deliver a value of the wrong type.
import k.drag.DragKind
import k.drag.DropTarget

struct CardDrag {
    let cardId: Int
}

struct ColumnDrag {
    let columnId: Int
}

let cardDrag: DragKind<CardDrag> = DragKind(name: "card")

component Board {
    func placeColumn(column: ColumnDrag) {}

    view {
        DropTarget(kind: cardDrag, label: "Column") {
            text { "Cards" }
        }
        .on(dropped: placeColumn)
    }
}
