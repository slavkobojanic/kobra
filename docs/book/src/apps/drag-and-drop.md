# Drag and drop

Drag and drop is three components in `k.drag`: **`Draggable`** makes its content a drag source,
**`DropTarget`** is a region that accepts drops, and **`ReorderableList`** is a list whose items
reorder within it and move to other lists. Every one of them works with a mouse, a pen, a finger and
the keyboard, and announces each keyboard step to screen readers. The design is in
`docs/design/41-drag-and-drop.md`.

## What is dragged: `DragKind`

A drag has a **kind**: a name paired with the type of its payload, the way a `StorageKey` pairs a name
with the type stored under it.

```k
// examples/kanban/src/board/cardDrag.k
import k.drag.DragKind

/// The drag of a card: every column's list has this kind, so cards move
/// between columns, and nothing else can be dropped on one.
public let cardDrag: DragKind<Card> = DragKind(name: "card")
```

Kinds with the same name are the same kind, so declare each kind once, next to its payload type, and
import it where it is used. Declaring a kind again with the same name and another payload type is a
compile error (`K2406`), since one kind's drop target would receive the other's payloads.

A region accepts only drags of its own kind, and its handler receives the kind's payload type, so a
drop can never deliver the wrong value. Both mistakes are compile errors:

```k,ignore
~import k.drag.Draggable
~import k.drag.DropTarget
Draggable(kind: cardDrag, payload: column, label: column.title) { … }   // error: a Column isn't a Card

DropTarget(kind: cardDrag, label: "Trash") { … }
    .on(dropped: { column: Column => … })                             // error: the payload is a Card
```

## Reordering lists

`ReorderableList` renders a `list` with one `item` per element, drawn by its `row` slot and kept by
`key`, like `for … key`. Lists of the same kind exchange items:

```k
// examples/kanban/src/pages/ColumnView.k
ReorderableList(kind: cardDrag, id: column.id.text, items: column.cards, key: { card => card.id.text }, label: column.title)
    .row { card =>
        if editing == card.id {
            CardEditor(board: board, card: card)
                .on(closed: { closed.emit() })
        } else {
            CardView(card: card, isDone: isDone)
                .on(edit: { edit.emit(card.id) })
        }
    }
    .itemLabel { card => card.title }
    .on(moved: { move => boards.moveCard(board: board, move: move) })
```

**The list never moves anything itself.** A drop fires `moved` once, on the list that receives the
item, with a `ListMove`: the item, the list and position it came from, and the list and position it
was dropped at. Your service applies it, and both lists redraw from the new state:

```k
// examples/kanban/src/board/BoardService.k
func moveCard(board: BoardId, move: ListMove<Card>) {
    guard let b = indexOfBoard(id: board) else {
        return
    }

    guard let from = indexOfColumn(board: b, text: move.fromList) else {
        return
    }

    guard let to = indexOfColumn(board: b, text: move.toList) else {
        return
    }
    // ...
    let card = boards[b].columns[from].cards.remove(at: move.fromIndex)
    var at = move.toIndex
    // ...
    boards[b].columns[to].cards.insert(card, at: at)
    changed(board: b)
}
```

`toIndex` is counted after the removal, so "remove, then insert" is right within one list and between
two. An item dropped on a row takes that row's position; dropped on the list's empty space, it goes to
the end. `accepts` refines which items a list takes from other lists:
`.accepts { card => column.cards.count < column.limit }`.

## Drop targets

```k
~import k.drag.DragKind
import k.drag.DropTarget
~
~struct Card implements Codable {
~    let id: Int
~    let isDone: Bool
~}
~
~let cardDrag: DragKind<Card> = DragKind(name: "card")
~
~service Boards {
~    func archive(id: Int) {}
~}
~
~component ArchiveZone {
~    @Inject let boards: Boards
~
~    view {

DropTarget(kind: cardDrag, label: "Archive") {
    text { "Drop a card here to archive it" }
}
.accepts { card => card.isDone }
.on(dropped: { card => boards.archive(id: card.id) })
~    }
~}
```

A row of a `ReorderableList` can be dropped on a `DropTarget` of its kind (a trash zone), and so can a
`Draggable`.

## Pointer, touch and keyboard

- **Mouse and pen:** a drag starts once the pointer moved 4px with the button down, so clicking a card
  is still a click. A copy of the element follows the pointer.
- **Touch:** a drag starts after a 250ms press, so a swipe still scrolls the page. Once it started, the
  page doesn't scroll under it.
- **Keyboard** (the WAI-ARIA alternative to dragging): every source is focusable. **Space** or
  **Enter** picks it up, **ArrowUp** and **ArrowDown** move it within its list, **ArrowLeft** and
  **ArrowRight** carry it to the previous or next list or drop target, **Space** or **Enter** drops it
  and **Escape** puts it back. Each step is announced ("Write tests: Position 2 of 3 in Doing"), and
  after a drop the moved item keeps the focus.
- Pressing a button, field or link inside a source never starts a drag, so a card's Edit button works.

While a drag is under way, the source has `data-k-dragging`, the region under it `data-k-drop-target`
and an outline, and a list shows a line where the item would land. The colors are the `dropTarget` and
`dropIndicator` tokens, which your theme sets with the assignment form: `dropIndicator = .primary`.

## Testing

Tests drag the way users do. `element.drag(to: other)` presses, moves and releases the mouse;
the keyboard path is `press`:

```k
// examples/kanban/src/pages/BoardPage.test.k
test "Space, ArrowRight and Space move a card to the next column" {
    let id = try seeded()
    let screen = render(BoardPage(id: id.value))
    let card = screen.list(named: "To do").item(containing: "A")

    card.press(.space)
    card.press(.arrowRight)
    card.press(.space)

    expect screen.list(named: "To do").items().count == 1
    expect screen.list(named: "Doing").items().count == 1
    // ...
}
// ...
test "a mouse drag onto the Done column finishes a card, and the progress follows" {
    let id = try seeded()
    let screen = render(BoardPage(id: id.value))

    screen.list(named: "To do").item(containing: "B").drag(to: screen.list(named: "Done"))

    expect screen.list(named: "Done").items().count == 1
    expect screen.text("Progress: 1 of 2 cards done").isVisible
    // ...
}
```

The Kanban example (`examples/kanban` in the repository, [its chapter](../examples/kanban.md)) uses all of it.
