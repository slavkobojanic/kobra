# Element References: Focus and Scrolling

## Decision

**A component names one of its own elements with an `ElementRef` field and the `.ref(…)` attribute, and its handlers move the focus to that element or scroll to it.** `ElementRef` is a plain class in `k.ui`; there are no strings, ids or selectors. This is the focus and scrolling row of [45's inventory](45-web-platform.md#inventory), Tier 2, and resolves 45's open question on element references.

```k
import k.ui.ElementRef

component NewTodoField {
    @Inject let todos: TodoService

    @State var title: String = ""

    let titleField = ElementRef()

    func add() {
        do {
            try todos.add(title: title)
            title = ""
        } catch let rejection {
            error = rejection
        }
        // Back to typing, whether Enter or the Add button submitted.
        titleField.focus()
    }

    view {
        form {
            textField(value: bind title)
                .label("New todo")
                .ref(titleField)

            button { "Add" }
                .type(.submit)
        }
        .on(submit: add)
    }
}
```

Five rules hold everywhere:

- **A reference is a `let` field of the component, page or layout whose element it names, initialized with `ElementRef()`,** and is attached with `.ref(…)` to exactly one element of that declaration's own view, outside any `for` body and any scoped slot. The compiler checks all of it (`K3113`, `K3114`), and warns about a reference that is never attached (`K3115`).
- **Operations take effect once the view has caught up:** at the end of the handler or effect that called them, after the state it changed has re-rendered. So a handler can open an editor and focus its field in the same breath. While its element isn't shown, an operation does nothing.
- **Only code that runs after rendering calls them:** handlers, effects, `init()`, `form.handle` actions and the functions they call. Calling one from a view, a `@Derived`, a `@Load` or another field initializer is `K3116`. So nothing runs on the server, and nothing moves the focus as a side effect of rendering.
- **The framework moves the focus itself in one place:** a `@Form` submit that fails puts the focus in the first invalid field ([below](#focus-the-framework-moves)). Everything else is explicit, because only the app knows where the user should land.
- **Tested without a browser:** `k test` screens report the focused element and the elements scrolled into view.

## `ElementRef`

```k
@Native
public class ElementRef {
    @Native
    init() {}

    func focus(selectingText: Bool = false)
    func blur()
    func scrollIntoView(alignment: ScrollAlignment = .nearest, animated: Bool = true)
}

public enum ScrollAlignment {
    case start
    case center
    case end
    case nearest
}
```

- **`focus()` moves the focus to the element.** An element that can't take the focus by itself (a heading, a section, an article) gets `tabindex="-1"` first, so it can take the focus from code without becoming a Tab stop: focusing the heading of what just appeared is the common case. `selectingText: true` also selects a text field's or text area's text (`select()`), for "fix this value" flows; elsewhere it selects nothing.
- **`blur()` takes the focus away** when the element has it.
- **`scrollIntoView(alignment:animated:)` scrolls the page, and every scrolling box around the element, until it is in view**, aligned on the block axis as `alignment` says (`.nearest` scrolls least). `animated: true` scrolls smoothly unless the user prefers reduced motion (`prefers-reduced-motion: reduce`), when it jumps; `animated: false` always jumps.
- **None of them is `@UserAction`.** Browsers move the focus and scroll from code without an activation, so they can be called after an `await` (focus the saved card once the rpc answers) or from an effect.
- **`.ref(_ reference: ElementRef)` is an attribute of every element.** It renders nothing: no attribute in the HTML, nothing on the server.

### The checks

| Code | When |
|---|---|
| `K3113` `misplaced-element-ref` (error) | An `ElementRef` field that isn't a plain `let` of a component, page or layout initialized with `ElementRef()` (a `var`, an annotated field such as `@Prop` or `@State`, a field of a service or struct); an `ElementRef()` anywhere else (a local, an argument); a `.ref(…)` whose argument isn't such a field of the declaration whose view it is. |
| `K3114` `element-ref-reused` (error) | A second `.ref(…)` of the same reference (label at the first), or one inside a `for` body or a scoped slot (a slot lambda with parameters, such as `ReorderableList`'s `.row { card => … }`), which can render the element any number of times. |
| `K3115` `unattached-element-ref` (warning) | A reference no `.ref(…)` attaches: its operations would never do anything. |
| `K3116` `element-ref-during-render` (error) | `focus`, `blur` or `scrollIntoView` called from code that may run while a view renders: a view, a `@Derived`, a `@Load` or another field initializer. |

A row of a list that needs its own reference becomes a component, which holds the reference: kanban's `CardView` does.

## When an operation runs

- **After the view has caught up.** A handler runs inside a batch, and the batch's end re-renders what it changed; the operation runs after that, in the same task. Called outside a batch (after an `await`), it runs after the re-render the writes before it scheduled. In the runtime it is a one-shot effect, so it runs after the render effects of the same flush.
- **Its element is the one the reference names when it runs**, not when it was called: an editor opened by the same handler is already there, a row removed by it isn't.
- **Nothing happens while the element isn't shown**, or isn't in the document. There is no error to handle: a focus target that vanished is not the user's problem.

## Focus the framework moves

| Moment | What happens | Automatic? |
|---|---|---|
| A control appears with `.autofocus(true)` | the focus moves into it ([05-views.md](05-views.md#attributes-are-per-element)) | yes, as before |
| A `@Form` submit fails, in the browser's validation or with a `ValidationError` the action throws | the focus moves to the first invalid field, in the form's field order (its struct's), which then describes its error with `aria-describedby` | **yes** |
| A client-side navigation shows another page | the focus moves to the new page's first level-1 heading (`tabindex="-1"`), so a screen reader announces the page | **yes** |
| An item was added; an editor or form closed; a dialog was dismissed | the app moves it with a reference: back to the field, to the button that opened the editor, to the new item | no, explicit |

- **Why the failed submit is automatic:** WCAG 3.3.1 and every form guideline ask for it, it has one right answer (the first error), and K already knows the fields and their errors. An app that wants otherwise has no reason to.
- **Why the rest is explicit:** after "Add", the right target is the field (todos) or the new item (a list editor); after closing an editor it is the control that opened it, which may have been re-created. A rule the framework guesses would be wrong for half the apps, and a wrong focus move is worse than none.
- **Scroll restoration on navigation is the router's** ([12-routing.md](12-routing.md#navigation)), not a reference's: back and forward restore the position, a new page starts at the top (or at its URL's `#fragment`), and a reload keeps the position. The same page updated in place with other parameters keeps the focus and the scroll, so a search field that writes the query never loses either.

## Server rendering and hydration

- **The server renders a referenced element exactly like any other;** `.ref(…)` adds nothing to the markup.
- **On the server `ElementRef()` is an inert value,** and its operations are never called there: only handlers, effects and `init()` call them (`K3116` keeps them out of everything else), and those run in the browser only. Server-only code can't hold one (`K3113`).
- **Hydration attaches the reference to the server's element,** so an effect that runs right after hydration finds it.

## Tests

`k test`'s screen has focus and scrolling, without layout:

| Member | Meaning |
|---|---|
| `Element.isFocused` | `true` when the element is the document's focused element |
| `Screen.focused` | a handle to the focused element of the view; `exists` is `false` when nothing in the view has the focus |
| `Element.wasScrolledIntoView` | `true` once code scrolled the element into view |

```k
test NewTodoField {
    test "the field takes the focus back after Add" {
        let screen = render(NewTodoField())

        screen.textField(labeled: "New todo").fill("Buy milk")
        screen.button(named: "Add").click()

        expect screen.textField(labeled: "New todo").isFocused
    }
}
```

An operation a click caused has run when `click()` returns, as the click's re-render has. The test DOM keeps the focus on an element removed from the view, as no browser does, so `isFocused` and `focused` count only elements still in it.

**The stdlib's own tests:** the checks in the sema corpus; the deferred chunk in the codegen corpus; `refs.test.js` against the fake DOM (deferral to the end of the batch, unattached and removed elements, `tabindex="-1"`, selection, alignment and reduced motion) and the failed-submit focus in `ui-forms.test.js`; and real Chrome: focus moves after an action, the first invalid field is focused on a failed submit, and `scrollIntoView` brings a far element into the viewport.

## Server and client

| Declaration | Side |
|---|---|
| `ElementRef`, `ScrollAlignment` | both (an inert value on the server) |

**The browser code is a deferred chunk,** `refs.[hash].js`, shipped only to clients that construct an `ElementRef`, and measured against its own budget. The shipped runtime chunk doesn't grow. The failed-submit focus is in the forms chunk.

## Examples

- **todos:** "Add" puts the focus back in the new-todo field, whether Enter or the button submitted, and after a rejected title too, so the user fixes it at once.
- **kanban:** closing a card's editor (Save, Cancel or Escape) puts the focus back on the card's "Edit" button, and closing the new-card form on the column's "Add a card" button, instead of dropping it on the page's body.

## Fullscreen

**Fullscreen joins later as an operation of `ElementRef`:** `enterFullscreen() async throws FullscreenError`, `@UserAction(consuming: true)` as 45 recorded, with a client-only `Fullscreen` service for `isActive` and `exit()`. It needs the reference this file adds and nothing else, so it isn't a new mechanism; it waits for an app that needs it (a video player beyond the native controls, a slideshow). Pointer lock would follow the same shape.

## As built

Built tests first, as designed above, with these decisions:

- **Declarations.** `k.ui` holds `ElementRef` and `ScrollAlignment`; `.ref(…)` is a universal attribute (`UNIVERSAL_ATTRIBUTES`). The checks are `crates/k_sema/src/element_refs.rs` (`K3113`–`K3115`, a pass after the bodies over the attributes they resolved) and `body.rs` (`K3116`, at the call).
- **Runtime.** `crates/k_runtime/js/runtime/refs.js`, a feature chunk of `k/ui`: the element binding calls the reference's `$attach(el)`, which detaches with the element's owner; each operation runs in a one-shot effect in a detached root. The server's `natives` table has an inert `ElementRef`.
- **The failed-submit focus** is in `forms.js`' `handle`, after both the browser's validation and a thrown `ValidationError`.
- **Tests.** The sema corpus (`tests/corpus/sema/views/element-ref*.k`), the `split/element-refs-ship-their-own-chunk` codegen case, `refs.test.js` and the failed-submit case of `ui-forms.test.js`, the `focusing` fixture (`k test`, and `chrome.rs`' `element_refs_move_the_focus_after_an_action_in_chrome`, `a_failed_submit_focuses_the_first_invalid_field_in_chrome` and `scroll_into_view_reaches_an_element_in_chrome`), and the examples' own tests in `k test` and Chrome (`todos_puts_the_focus_back_in_the_field_after_clicking_add`, `kanban_returns_the_focus_to_the_card_after_closing_its_editor`).
- **`Screen.focused` asks each element of the view about its own document,** not the global one: in the test DOM a node cloned from a template keeps the template's document as its owner, and that is the document whose `activeElement` its `focus()` sets.
- **Sizes.** The refs chunk is 434 B gzip compacted (budget 512 B, `REFS_SIZE_BUDGET`); the forms chunk went from 1,807 B to 1,971 B with the failed-submit focus (budget 2 KB). The shipped full client runtime didn't grow (10,238 B of 10,240 B).
- **Kanban's card** gets its focus back through a prop: `BoardPage` remembers the card whose editor closed last (`lastEdited`), and its `CardView`, created again when the editor closes, focuses its Edit button from an `@Effect(on: [])` when `takesFocus` is set. The new-card form's "Add a card" is in `ColumnView`'s own view, so its `closed` handler focuses it directly.
- **Navigation focus and scroll restoration** (built in the router's next round, with room made in the full client runtime first, docs/compiler/performance.md §11.3): `router.js`' `arrive` runs once a navigation is shown. When another page was mounted it focuses the container's first `h1` with `preventScroll`, giving it `tabindex="-1"` unless it has one; it never moves the focus for the first page (the browser placed it) or for an update in place. The router sets `history.scrollRestoration = "manual"` and keeps each page's position per history entry: in memory (each entry it pushes carries its index), and in the entry's state before a push and on `pagehide`, which a reload keeps; a push starts at the top or at the fragment's element. Decided while building: **the first `h1` is searched in the whole router container,** layouts included, as the rule says ("the first level-1 heading"), and **a page still `loading` has no heading to focus yet**: the focus stays where the click left it (the body, once the link is gone), since a focus that jumps when the data arrives would be worse. Tests: `ui-router.test.js` ("focus and scroll": the first page, a navigation, an own `tabindex`, a fragment, back and forward, an update in place, a reload, back after a reload) and `chrome.rs`' `navigation_focuses_the_heading_and_restores_the_scroll_in_chrome` (the `navigating` fixture, served by `k dev`). **Not a `k test` case:** `k test`'s screen renders one page without the router ([21-testing.md](21-testing.md), "the screen keeps showing the view it rendered"), so there is no navigation to observe; the router's own tests are the JS ones and Chrome. Sizes: the full client runtime went from 9,306 B to 9,723 B gzip with this and the plain `bind` round (budget 10,240 B); a release router ships about 300 B more gzip (shop 29,952 B against 30,500 B, after the codec round of performance.md §11.3).

## Why

- **A field plus an attribute** names an element with ordinary, checked names: a typo is a compile error, renaming works, and the IDE finds every use, which string ids can't give.
- **Owned by the component, attached once, never in a loop:** a reference always means one element, so an operation can never pick the wrong one, and a list's rows hold their own as components.
- **Deferred to the end of the handler:** the case that matters most, "open it and focus it", works without the app knowing about render timing.
- **Few operations:** focus, blur and scrolling are what accessibility needs; measuring elements is `resized`'s ([45](45-web-platform.md#element-events)), and anything else would be DOM access K deliberately doesn't give.

## Alternatives rejected

- **String ids** (`.id("title")` and `focus(id:)`): unchecked, collide between two instances of a component, and invite treating the DOM as an API.
- **Callback refs** (`.ref { el in … }`, as in React): hand app code a DOM element, which K has no type for.
- **A reference in `@State`, or passed to children as a prop** (`forwardRef`): a reference that travels can name an element of another component's view, which no check can follow; a child that offers a focus target can take a `@Prop` saying when to focus, as kanban's `CardView` does.
- **Typed references by element kind** (`ElementRef<TextField>` for `select()`): one more type parameter for one operation; `focus(selectingText:)` covers it.
- **Moving the focus automatically whenever its element is removed:** the right target depends on the app; see [focus the framework moves](#focus-the-framework-moves).

## Open questions

- **Keyed references,** `ElementRefs<Key>` with `.ref(rows, key: card.id)`, to focus one row of a `for` without a component per row.
- **One reference on exclusive branches** (`if editing { field.ref(r) } else { button.ref(r) }`), allowed only when the compiler can see they never coexist.
- **A component's focus target:** `CardView(…).ref(r)` focusing the child's chosen element.
- **Navigation focus and scroll restoration,** built in the router ([As built](#as-built)).
- **Focus traps and focus return for `Dialog`** come with the accessible widgets ([05-views.md](05-views.md)).
- **Scrolling a box to a position** (`scrollTo(top:)`), and reading whether an element has the focus reactively.
