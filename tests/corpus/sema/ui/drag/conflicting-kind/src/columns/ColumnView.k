import board.Card
import k.drag.DragKind

/// The same kind again.
let sameCardDrag: DragKind<Card> = DragKind(name: "card")

/// Another payload under the card kind's name.
let columnDrag: DragKind<Int> = DragKind(name: "card")

/// A column of the board.
public component ColumnView {
    private let listKind: DragKind<String> = DragKind(name: "card")

    view {
        text { "Column" }
    }
}
