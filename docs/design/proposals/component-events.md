# Proposal: Listening to Component Events (`@Emits`)

> **Status: proposal.** Not yet accepted into the design. Matches how `examples/notes` uses `@Emits` (`NoteCard(note: note).on(pinToggled: togglePin)`), and first written down for the language guide in `docs/book`.

## Problem

[04-components.md](../04-components.md) declares events with `@Emits let changed: Event<Int>` and fires them with `changed.emit(count)`, but doesn't say how a parent listens. [05-views.md](../05-views.md) says events on built-in elements are chained with `.on(click: handler)`.

## Proposal

**A parent listens to a component's event with the same chained `.on(name: handler)` used for element events,** where `name` is the `@Emits` field:

```k
component Counter {
    @Emits let changed: Event<Int>
    @State var count: Int = 0

    func increment() {
        count += 1
        changed.emit(count)
    }

    view {
        button { "Add" }
            .on(click: increment)
    }
}

Counter()
    .on(changed: { value => Log.info("count is now {value}") })
```

- **The handler's type follows the event's payload.** For `Event<T>` it is a function taking a `T`: a lambda with one parameter, or a function passed by name (`.on(changed: record)`). An event without a payload is declared `Event<Void>`, fired with `emit()`, and handled by a lambda without parameters (`.on(closed: { isOpen = false })`).
- **Handlers may be `async`.** As for element events, the component doesn't wait for them.
- **Listening is optional.** An event nobody listens to does nothing, so events are never required arguments. `.on(…)` is a chained call, following the named-or-chained rule ([01-principles.md](../01-principles.md)).
- **Each event is listened to at most once per call site.** Writing `.on(changed: …)` twice on the same component is a compile error, like setting any option twice ([04-components.md](../04-components.md)). Two different events are two `.on` calls.
- **Unknown event names are compile errors**, with a hint listing the component's `@Emits` fields.
- `emit` can be called only from inside the declaring component. A parent can't fire a child's event.

## Why

- One form for "something happened" across elements and components keeps "one obvious way": a reader sees `.on(…)` and knows it is an event, whatever emits it.
- Events stay distinct from `@Binding`, which is for state the parent shares ([04-components.md](../04-components.md)).

## Open questions

- Whether a built-in element may take several events in one call (`.on(click: f, keyDown: g)`), or one `.on` per event as for components.
