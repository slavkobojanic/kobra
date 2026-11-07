import k.drag.DragKind

/// The drag of a card: every column's list has this kind, so cards move
/// between columns, and nothing else can be dropped on one.
public let cardDrag: DragKind<Card> = DragKind(name: "card")
