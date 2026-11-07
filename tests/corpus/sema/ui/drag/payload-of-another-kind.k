// The payload must have the kind's type: a `Draggable` of `cardDrag` can't carry a `ColumnDrag`
// (K2001 at the payload).
import k.drag.DragKind
import k.drag.Draggable

struct CardDrag {
    let cardId: Int
}

struct ColumnDrag {
    let columnId: Int
}

let cardDrag: DragKind<CardDrag> = DragKind(name: "card")

component Column {
    view {
        Draggable(kind: cardDrag, payload: ColumnDrag(columnId: 1), label: "Column 1") {
            text { "Column 1" }
        }
    }
}
