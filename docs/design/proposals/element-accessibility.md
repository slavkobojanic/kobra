# Proposal: Checkbox Binding and Accessibility Attributes

> **Status: proposal.** Not yet accepted into the design. First used by `examples/todos`.

## Problem

[05-views.md](../05-views.md) lists `checkbox` among the input elements and shows `textField(value: bind …)` with `.label(…)` ([15-forms-and-validation.md](../15-forms-and-validation.md)), but doesn't define the checkbox's bound value, how a non-field element gets an accessible name, or how the ARIA states that widgets need (pressed, live regions) are written. Without them an accessible toggle button or a status message needs untyped attributes, which the design rules out.

## Proposal

### `checkbox(isChecked:)`

```k
checkbox(isChecked: bind todo.isDone)
    .label("Done")
```

- The required argument is the bound `Bool`, named `isChecked` after the property it reflects. As with every input, it takes `bind x` or a custom `Binding` ([04-components.md](../04-components.md)).
- `.label(…)` renders a visible `<label>` associated with the input, exactly as for `textField`. Tests find it with `checkbox(labeled:)` ([testing-support.md](testing-support.md)).

### Accessible names: `.label` on any element

- On a form control, `.label("…")` is the visible label (as already designed).
- **On any other element, `.label("…")` sets the accessible name** (`aria-label`), for landmarks and groups that have no visible heading: `nav { … }.label("Filter todos")`.
- There is one modifier for "the name assistive technology announces", whatever the element. The compiler picks `<label for>` or `aria-label`, and warns when a landmark such as a second `nav` has no name.

### ARIA states and roles are typed modifiers

| Modifier | Compiles to | Allowed on |
|---|---|---|
| `.pressed(Bool)` | `aria-pressed` | `button` |
| `.expanded(Bool)` | `aria-expanded` | `button`, `link` |
| `.role(.alert)` / `.role(.status)` | `role` (both are live regions) | non-interactive elements |

- Each takes a typed value. `.pressed` on a `box` is a compile error, because only buttons can be pressed.
- `.role` accepts only roles the element doesn't already have and that can't be expressed with a semantic element. `section { }.role(.navigation)` is a compile error with the hint "use `nav`".
- `.placeholder("…")` and `.type(.submit / .button / .reset)` set the HTML attributes of the same name. `.type(.button)` is needed on a non-submitting button inside a `form`.

## Why

- `isChecked` follows the Swift-style `is` prefix for booleans that the design already uses (`isOpen`, `isExpanded`).
- One `.label` modifier for every accessible name keeps "one obvious way", and maps directly to the accessibility queries tests use.
- Typed ARIA modifiers move common accessibility mistakes (a toggle without `aria-pressed`, a redundant role) from audits to compile errors.

## Open questions

- Visually hidden text (for example "Delete *Buy milk*" read aloud while only "Delete" is shown) needs either a `.label` that overrides button content, or a stdlib `VisuallyHidden` element. The todos example avoids the question by using scoped queries in tests, but a real app would want one or the other.
