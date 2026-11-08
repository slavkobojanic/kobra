// A drag kind's name is a string literal (41-drag-and-drop.md), like a storage key's: anything else is
// K2403, with no fix, as `DragKind` has no `parse`.
import k.drag.DragKind

struct Card {
    let id: Int
}

let cards: DragKind<Card> = DragKind(name: "card")

func kind(named name: String) -> DragKind<Card> {
    return DragKind(name: name)
}
