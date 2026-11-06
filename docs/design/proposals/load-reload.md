# Proposal: Reloading `@Load` Fields

> **Status: proposal.** Not yet accepted into the design. First used by `examples/weather`.

## Problem

[14-async-data.md](../14-async-data.md) says a `@Load` field re-runs when its inputs change. It doesn't say how to re-run loads whose inputs *haven't* changed (a retry button in `failed`, a periodic refresh), or what the view shows while a re-run is in flight. The example `ErrorBanner(...).retry(load)` in [05-views.md](../05-views.md) assumes something to retry with.

## Proposal

### `reload()`

Every page, layout or component that declares `@Load` fields gets a generated method **`reload()`**. It re-runs all of the declaration's loads in dependency order.

```k
failed(let error) {
    button { "Try again" }
        .on(click: reload)
}

@Effect(on: [placeId])
func refreshPeriodically() {
    let timer = Timer.every(10.minutes) { reload() }

    onCleanup { timer.cancel() }
}
```

- `reload()` is synchronous. It starts the re-run and returns. Any run still in flight is cancelled first, as for an input change.
- It re-runs every load, not a chosen subset. Loads that are cheap to repeat (a place that never changes) should be cached in their service, where caching belongs ([10-services.md](../10-services.md)).
- Declaring a member named `reload` in a declaration with `@Load` fields is a compile error.

### What renders during a re-run

| Situation | Renders |
|---|---|
| First load, nothing resolved yet | `loading` |
| An **input changed** (for example a different `@Param`) | `loading`, because the old values describe something else |
| **`reload()`** with every load previously resolved | `view`, with the previous values, until all new values resolve. The swap is atomic, so the view never mixes old and new values. |
| `reload()` from `failed` | `loading` |
| Any re-run throws | `failed` |

- **Atomic swap:** during a reload, the view keeps every `@Load` value from the previous run until all loads have finished. Then all are replaced at once. Dependent values stay consistent with each other (a forecast always belongs to the place shown).
- **Loading values are serializable.** A `@Load` field's type must implement `Codable`, because server rendering serializes it to the client ([14-async-data.md](../14-async-data.md)). Any other type is a compile error at the field.

## Open questions

- Whether pages want an `isReloading: Bool` to show a subtle "refreshing" indicator. The weather example doesn't need one.
- Whether a failed *reload* should keep the stale view and expose the error, instead of switching to `failed`. That would be friendlier for dashboards, but it adds a second error path to every page.
