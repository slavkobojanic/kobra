# Keyboard shortcuts

A single-key shortcut for a page is the **`KeyboardShortcut` component in `k.ui`**. While it is
mounted, pressing its key fires its `pressed` event. It renders nothing. The design is in
`docs/design/43-keyboard-shortcuts.md`.

```k
// examples/kanban/src/pages/BoardPage.k
view {
    main {
        KeyboardShortcut(key: "n", label: "Add a card to the first column")
            .on(pressed: startNewCard)

        KeyboardShortcut(key: "e", label: "Export the board")
            .on(pressed: exportBoard)

        KeyboardShortcut(key: "?", label: "Show keyboard shortcuts")
            .on(pressed: { isShowingShortcuts = not isShowingShortcuts })

        KeyboardShortcut(key: "Escape", label: "Close the editor")
            .on(pressed: closeEditor)
        // ...
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
// examples/kanban/src/pages/BoardPage.test.k
screen.heading(named: "Seed").press(.character("n"))
screen.textField(labeled: "New card in To do").fill("")
```
