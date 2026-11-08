# Drag and Drop

## Decision

Drag and drop is **three stdlib components in `k.drag`**, not element modifiers: `Draggable` makes its content a drag source, `DropTarget` is a region that accepts drops, and `ReorderableList` is a list whose items reorder within it and move between lists. **What is dragged is typed by a `DragKind<T>`**, a name paired with the payload type, the way `StorageKey<T>` pairs a name with a stored type ([36-local-storage.md](36-local-storage.md#keys)). Every component works with a pointer (mouse, pen, touch) and with the **keyboard**, which is the WAI-ARIA alternative to dragging, with each step announced to screen readers.

```k
import k.drag.DragKind
import k.drag.ListMove
import k.drag.ReorderableList

// Cards move between columns; the payload of a card drag is the card.
let cardDrag: DragKind<Card> = DragKind(name: "card")

component ColumnView {
    @Prop let column: Column

    @Inject let board: BoardService

    view {
        ReorderableList(kind: cardDrag, id: column.id.text, items: column.cards, key: { card => card.id.text }, label: column.title)
            .row { card =>
                CardView(card: card)
            }
            .itemLabel { card => card.title }
            .accepts { card => column.cards.count < column.limit }
            .on(moved: { move => board.move(card: move) })
    }
}
```

## Kinds and payloads

```k
@Native
public struct DragKind<T> {
    @Native
    init(name: String) {}

    var name: String
}
```

- **A kind pairs a name with the payload type,** given by the binding as generic arguments always are ([RA-23 in the grammar](../compiler/grammar.md)). `Draggable(kind:payload:)` requires a payload of the kind's type, and a drop handler receives exactly that type, so a drop can't deliver the wrong value: both mistakes are type errors (K2001).
- **A drop target accepts only drags of its own kind.** The kind is the coarse, compile-time accept rule; `accepts` ([below](#drop-targets-and-accept-rules)) is the fine, run-time one.
- **Kinds with the same name are the same kind.** The name is a string literal, declared once (usually a top-level `let` next to the payload type).
- **Two kinds with the same name must carry the same payload type.** A drop target of one would otherwise accept the other's drags and hand its handler a payload of the wrong type, which the types can't catch because the two kinds look unrelated. A `DragKind` constant or field named like an earlier one (in file path order) with a different payload type is a compile error (`K2406`) at its name literal, with a label at the earlier kind, like a storage key named twice (`K2405`, [36-local-storage.md](36-local-storage.md#keys)). Redeclaring a kind with the same name and payload type is allowed, since it is the same kind. Test code counts too: unlike a storage key, which a test may name again to inspect what a service stored, a kind has no reason to change its payload in a test.
- **The payload is a value,** copied into the drag when it starts, so a later change to the source doesn't change what is dropped. Payloads stay in the page: dragging between windows or applications is not supported ([open questions](#open-questions)).

## `Draggable`

```k
Draggable(kind: cardDrag, payload: card, label: card.title) {
    CardView(card: card)
}
```

| Member | Meaning |
|---|---|
| `content: View` (trailing block) | what is dragged |
| `kind: DragKind<T>` | the kind of drag |
| `payload: T` | the value a drop delivers |
| `label: String` | the name announced for the content ("Write tests"), and the group's accessible name |

It renders its content in a focusable `div` with `role="group"`, `aria-roledescription="draggable"`, `aria-label` set to `label`, and `aria-describedby` pointing at the one hidden help text every source shares.

## Drop targets and accept rules

```k
DropTarget(kind: cardDrag, label: "Archive") {
    Icon(name: .archive)

    text { "Drop a card here to archive it" }
}
.accepts { card => card.isDone }
.on(dropped: { card => board.archive(id: card.id) })
```

| Member | Meaning |
|---|---|
| `content: View` (trailing block) | the region's content |
| `kind: DragKind<T>` | the kind it accepts |
| `label: String` | its accessible name (a `group`), announced while a drag is over it |
| `accepts: (T) -> Bool` (chained) | the accept rule; every payload of the kind by default |
| `dropped: Event<T>` | fires with the payload of an accepted drop |

- **A payload the rule rejects can't be dropped there,** with the pointer or the keyboard: the region isn't highlighted under the pointer, a release over it cancels, and keyboard navigation skips it.
- `accepts` is a plain function prop, read when a drag reaches the region, so a rule over state (`column.cards.count < limit`) is current at every drag.

## Reorderable lists

`ReorderableList<T>` renders a `list` named `label`, with one `item` per element of `items`, drawn by the scoped `row` slot and **kept by `key`**, like `for … key` ([05-views.md](05-views.md#control-flow)).

| Member | Meaning |
|---|---|
| `row: (T) -> View` (chained, required) | draws one item |
| `kind: DragKind<T>` | the items' kind; lists of one kind exchange items |
| `id: String` | identifies the list in a `ListMove` |
| `items: List<T>` | the items, in order |
| `key: (T) -> String` | each item's identity |
| `label: String` | the list's accessible name ("To do") |
| `itemLabel: (T) -> String` (chained) | each item's name in announcements; its key when empty, as it is by default |
| `accepts: (T) -> Bool` (chained) | the accept rule for items coming from another list; a list always takes back its own item |
| `moved: Event<ListMove<T>>` | fires on the list that receives an item |

```k
public struct ListMove<T> {
    let item: T
    let fromList: String
    let fromIndex: Int
    let toList: String
    let toIndex: Int
}
```

- **The list never moves anything itself.** A drop fires `moved` once, on the receiving list, and the app changes its state: remove the item at `fromIndex` of `fromList`, then insert it at `toIndex` of `toList`. Both lists redraw from the new state. One source of truth stays in the app's services, as everywhere else ([03-reactivity.md](03-reactivity.md)).
- **`toIndex` is counted after the removal,** so the two steps above are always right, within one list or between two. Dropping an item where it was fires nothing.
- **An item dropped on a row takes that row's position;** dropped on the list's empty space, it goes to the end. Neither needs layout, so the semantics are the same with a mouse, a finger, the keyboard and in tests.
- A row of a list can also be dropped on a `DropTarget` of the same kind (a trash zone), which fires the target's `dropped` and nothing on the lists. A `Draggable` can't be dropped on a list: a list's `moved` always has a `fromList`.
- `row` is the required slot and is chained (`.row { card => … }`), because a scoped slot with parameters can't be the trailing block.

## Pointer and touch

- **Pointer events serve the mouse, pens and touch alike.** The HTML drag-and-drop API is not used: it has no touch support on most mobile browsers, no keyboard story, payloads only as strings, and drag images the page can't style.
- **A mouse or pen drag starts after the pointer moves 4px** with the button down, so a click on a card is still a click, and the click that ends a drag is swallowed.
- **A touch drag starts after a 250ms press that moved less than 8px.** A swipe therefore still scrolls the page; once the drag has started, the page doesn't scroll under it. Sources set `touch-action: manipulation` and turn off text selection and the long-press callout.
- **Pressing a control inside a source (a button, a field, a link) never starts a drag,** with the pointer or the keyboard, so a card's Edit button works as usual.
- A copy of the dragged element follows the pointer, ignoring the pointer itself; the region under the pointer is found by hit testing. A cancelled pointer (the browser took the gesture over) and Escape cancel the drag.

## Keyboard and screen readers

Every source is focusable and is described by one shared hidden help text: "Press Space or Enter to pick up. While carrying, the arrow keys move, Space or Enter drops and Escape cancels." The interaction follows the WAI-ARIA Authoring Practices' guidance for drag and drop, since `aria-grabbed` and `aria-dropeffect` are deprecated:

| Key | While not carrying | While carrying |
|---|---|---|
| Space, Enter | picks the focused source up | drops it where it is |
| ArrowUp, ArrowDown | (the page's) | moves the item one position within its list; for a `Draggable`, the previous or next region |
| ArrowLeft, ArrowRight | (the page's) | the previous or next list or drop target that accepts it, in document order, wrapping around |
| Escape | (the page's) | cancels |
| Tab, or anything that moves the focus | (the page's) | cancels |

- **Each step is announced** in an assertive live region: "Picked up Write tests. Position 1 of 3 in To do.", "Write tests: Position 2 of 2 in Done.", "Dropped Write tests. Position 2 of 2 in Done.", "Cancelled. Write tests is back where it was."
- Moving between lists keeps the position when the next list has it, and goes to its end otherwise.
- **After a keyboard drop the moved item has the focus,** in its new list, so the user can carry on.
- A list row keeps its `listitem` role (a screen reader still says "2 of 5"); the help text and the announcements carry the rest.

## While dragging

Nothing moves until the drop. The page shows where the drop would land:

- the source carries the `data-k-dragging` attribute;
- the region under the drag, when it accepts it, carries `data-k-drop-target` and an outline in the `dropTarget` token;
- a list shows a line, in the `dropIndicator` token, where the item would land.

`k.drag` declares the tokens it uses, with defaults, as libraries do ([07-themes-and-units.md](07-themes-and-units.md#library-tokens)); an app overrides them with the assignment form in its theme (`dropIndicator = #16A34A`):

| Token | Used for |
|---|---|
| `color dropIndicator` | the line where a list item would land |
| `color dropTarget` | the outline of the region under the drag |

## Server rendering

The components render on the server like any other: the same elements and attributes, with the slot content inside, so a server-rendered list is readable before the client starts, and the browser claims it while hydrating. Dragging starts working after hydration.

## Tests

`k.test` drives drags the way a user does ([21-testing.md](21-testing.md)):

- **`element.drag(to: other)`** presses the pointer on `element`, moves it onto `other` and releases it there, so a test reads like the gesture: `screen.item(containing: "Write tests").drag(to: screen.list(named: "Done"))`.
- **The keyboard path uses `press`,** with `Key` gaining `.arrowLeft` and `.arrowRight`: `card.press(.space)`, `card.press(.arrowRight)`, `card.press(.space)`.
- `screen.list(named:)` finds a list by its accessible name, which every `ReorderableList` has.

```k
test "a card moves to the next column with the keyboard" {
    let screen = render(BoardPage())
    let card = screen.item(containing: "Write tests")

    card.press(.space)
    card.press(.arrowRight)
    card.press(.space)

    expect screen.list(named: "Done").items().count == 1
}
```

## Why

- **Components rather than modifiers.** A reorderable list needs to own its rows (to know each one's key and position, to place an indicator, to give the focus back), which a modifier on arbitrary elements can't. Components also keep drag and drop in a chunk only the apps that use it download.
- **A typed kind** turns "this target accepts that drag" into a type, and the payload into a value of a known type, instead of strings in a `dataTransfer`.
- **The app applies the move** because the list's items are app state; a list that reordered its own copy would be a second source of truth.
- **Position-taking semantics** need no geometry, so a drag means the same with every input and in the fake DOM of tests.
- **Keyboard first-class:** the same events, the same payloads, with announcements, so nothing about a board needs a mouse.

## Open questions

- **Files dragged from the desktop.** A `FileDropTarget` delivering `List<File>` ([42-files.md](42-files.md)) would complete file import.
- **Between windows.** Payloads that are `Codable` could cross windows through the HTML drag-and-drop API's `dataTransfer`.
- **Localized announcements.** The texts are English; `k.i18n` should supply them.
- **Auto-scrolling** a long list or the page while a pointer drag nears an edge.
- **Grids and two-dimensional reordering.**
