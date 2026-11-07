# Keyboard shortcuts

A single-key shortcut for a page is the **`KeyboardShortcut` component in `k.ui`**. While it is
mounted, pressing its key fires its `pressed` event. It renders nothing. The design is in
[43-keyboard-shortcuts.md](../../../design/43-keyboard-shortcuts.md).

```k
import k.ui.KeyboardShortcut

view {
    KeyboardShortcut(key: "n", label: "New card")
        .on(pressed: startNewCard)

    KeyboardShortcut(key: "?", label: "Show keyboard shortcuts")
        .on(pressed: { isShowingShortcuts = not isShowingShortcuts })

    button { "New card (N)" }
        .on(click: startNewCard)
}
```

- `key` is the key as the browser names it: a character (`"n"`, `"?"`) or a key name (`"Escape"`).
  Letters match in either case.
- A key is **not** a shortcut while the user types in a field, when Control, Alt or Meta is held, or
  when an element already handled it (a keyboard drag's Space and arrows, a dialog's Escape).
- The shortcut belongs to its component: it works while the component is on the page, and leaving the
  page turns it off.
- Shortcuts are accelerators. Keep a visible control for every one, show the key in it ("New card
  (N)"), and list the page's shortcuts somewhere the user can find them.

In tests, `Key.character(_)` presses the key a user would:

```k
screen.heading(named: "Launch").press(.character("n"))

expect screen.textField(labeled: "New card in To do").exists
```
