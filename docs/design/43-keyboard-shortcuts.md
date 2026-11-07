# Keyboard Shortcuts

## Decision

A page-wide, single-key shortcut is the **`KeyboardShortcut` component in `k.ui`**: while it is mounted, pressing its key fires its `pressed` event. It renders nothing. Shortcuts are an accelerator, never the only way to do something: the control that does the same thing stays on the page.

```k
import k.ui.KeyboardShortcut

view {
    KeyboardShortcut(key: "n", label: "New card")
        .on(pressed: startNewCard)

    KeyboardShortcut(key: "?", label: "Show keyboard shortcuts")
        .on(pressed: { isShowingShortcuts = true })

    button { "New card (N)" }
        .on(click: startNewCard)
}
```

## Rules

- **`key` is the key as the browser names it** (`KeyboardEvent.key`): a character (`"n"`, `"?"`, `"/"`) or a key name (`"Escape"`). A letter matches in either case, so Shift doesn't defeat it; other characters match exactly, so `"?"` is Shift+/ on a US layout and whatever types `?` elsewhere.
- **A key is a shortcut only when nothing else wanted it:**
  - not while the user types in a `textField`, `textArea`, `dropdown` or other text control;
  - not with Control, Alt or Meta held, which belong to the browser and the system;
  - not when an element already handled the key. A keyboard drag's Space and arrows ([41-drag-and-drop.md](41-drag-and-drop.md#keyboard-and-screen-readers)) and a dialog's Escape win over a shortcut.
- **A handled shortcut doesn't type its key** anywhere else.
- **Single keys only.** Chords (`g` then `b`) and modifier combinations are [open questions](#open-questions); single keys are what WCAG 2.1.4 (character key shortcuts) asks to be easy to avoid, and a shortcut that lives with a component is turned off by leaving the page that mounts it.
- **The shortcut lives with its component.** Mounting it turns it on and unmounting turns it off, so a page's shortcuts never leak into another page, and a component that offers an action can offer its shortcut next to it.
- `label` says what the shortcut does. The page shows its shortcuts to the user (a "Keyboard shortcuts" list), and the control the shortcut stands for says its key in its visible text, so the shortcut can be discovered.

## Tests

`Key` has a `.character(String)` case, so a test presses the key a user would: `screen.button(named: "New card (N)").press(.character("n"))`. A shortcut listens on the whole page, so any element can receive the press.

## Why

- **A component** follows the rest of the view model: something that exists while a part of the page exists is declared in that part's view, and events reach the app with `.on(…)` ([04-components.md](04-components.md#events)). It needs no new event type on elements and no global registry.
- **The filters** (typing, modifiers, already handled) are the ones every app otherwise reimplements, usually incompletely: a shortcut that fires while typing a card title is the classic bug.
- Shipping its few lines in a chunk of their own keeps apps without shortcuts from paying for them.

## Open questions

- **Chords and modifiers** (`g b`, Control+K), and a way to turn character shortcuts off or remap them (WCAG 2.1.4).
- **A `keyDown` event on elements,** with a typed key, for widgets that handle keys while focused.
