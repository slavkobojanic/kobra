# Timers and the Clock

## Decision

**The current time, timers and sleeps come only from the injected `Clock` service** in `k.time`. Code that needs the time writes `@Inject let clock: Clock`; there is no ambient way to read it. Tests get a virtual clock by default, so they control time and never wait in real time.

```k
@Inject let clock: Clock

@Effect(on: [placeId])
func refreshPeriodically() {
    let timer = clock.every(10.minutes) { reload() }

    onCleanup { timer.cancel() }
}
```

## `Clock`

`Clock` is a stdlib service, usable on both sides:

| Member | Meaning |
|---|---|
| `clock.now: Date` | the current instant |
| `clock.every(_ interval: Duration, _ action: () -> Void) -> TimerHandle` | runs `action` every `interval`; the first run is after one interval |
| `clock.after(_ delay: Duration, _ action: () -> Void) -> TimerHandle` | runs `action` once, after `delay` |
| `clock.sleep(_ duration: Duration) async` | suspends the current async function |

```k
let timer = clock.every(10.minutes) { reload() }     // repeats; first fire after one interval
let once = clock.after(2.seconds) { dismiss() }      // fires once
timer.cancel()                                        // idempotent

await clock.sleep(300ms)                              // suspends the current async function
```

- **It is a service, so layering allows it where services go:** components, pages, layouts, services, endpoint pages and jobs may inject it ([10-services.md](10-services.md#layering)). It holds no request state, so services in every scope may inject it, `.server` included.
- **Plain functions and structs don't read the time.** They inject nothing, so code that needs the current instant takes it as a parameter, usually named `now`:

  ```k
  struct StripeSignature {
      let signedAt: Date

      func isFresh(now: Date, tolerance: Duration) -> Bool {
          return now - signedAt <= tolerance
      }
  }

  // in the service that injects the clock
  guard signature.isFresh(now: clock.now, tolerance: config.tolerance) else { … }
  ```

- **There is no `Date.now`, `.ago` or `.fromNow`.** "Ten minutes ago" is `clock.now - 10.minutes`, and "in three days" is `clock.now + 3.days`. Durations themselves need no clock: `10.minutes` is a `Duration` ([07-themes-and-units.md](07-themes-and-units.md#number-extensions)).
- `every` and `after` return a `TimerHandle` with `cancel()`.
- **Timers belong to their owner.** A timer started by a component (in an effect, a handler or a lambda) stops when the component is disposed: its action is bound to the component, and the browser `Clock` cancels the timer when the component goes away ([03-reactivity.md](03-reactivity.md#what-a-component-leaves-behind)). A timer started inside an `@Effect` should still be cancelled in `onCleanup`, so a re-run doesn't stack a second timer on the first. The compiler warns when a `TimerHandle` returned inside an effect is discarded.
- **`clock.sleep` is cancellable.** When the async effect or `@Load` that is sleeping is cancelled, the sleep ends and the function never resumes. Debouncing is therefore an async effect that sleeps first, and that handles its own errors, as every effect must ([03-reactivity.md](03-reactivity.md#effects-handle-their-own-errors)):

  ```k
  @Effect
  func search() async {
      let text = query

      await clock.sleep(300ms)              // a new keystroke cancels this run

      do {
          results = .loaded(try await places.search(text))
      } catch let error {
          results = .failed(error)
      }
  }
  ```

- **Effects don't run during server rendering,** so neither do timers started in them. Server-side work on a schedule is a background job ([30-background-jobs.md](30-background-jobs.md#schedules)).

## Relative times

Relative formatting takes the reference instant explicitly, so it reads the clock where the clock is injected:

```k
text { message.sentAt.formatted(as: .relative, from: clock.now) }     // "5 minutes ago"
```

**`clock.now` is not reactive.** Reading it in a view or a `@Derived` value doesn't subscribe to time, so "5 minutes ago" is computed when the view renders and doesn't change by itself. To show a live relative time, a component keeps the reference instant in `@State` and updates it with `clock.every`:

```k
component SentAt {
    @Prop let sentAt: Date
    @Inject let clock: Clock

    @State var now: Date = clock.now

    @Effect(on: [])
    func tick() {
        let timer = clock.every(1.minute) { now = clock.now }

        onCleanup { timer.cancel() }
    }

    view {
        text { sentAt.formatted(as: .relative, from: now) }
    }
}
```

- **`date.formatted(as: .relative, from: reference)`** describes `date` relative to `reference`: "5 minutes ago", "in 3 days". The `from:` argument is required.
- The other styles (`.date`, `.time`, `.dateTime`) need no reference instant.

## Testing with `TestClock`

```k
test CityWeather {
    use FakeWeatherService for WeatherService

    @Inject let weather: FakeWeatherService
    @Inject let clock: TestClock

    test "the forecast refreshes every ten minutes" {
        let screen = render(CityWeather(placeId: Place.berlin.id))
        await screen.settle()

        clock.advance(by: 10.minutes)
        await screen.settle()

        expect weather.forecastCalls == 2
    }
}
```

- **`TestClock` is the default for `Clock` in every `k test`** ([21-testing.md](21-testing.md#test-doubles-and-defaults)). A test reaches it with `@Inject let clock: TestClock` and never writes a `use` line for it.
- **Virtual time starts at 2026-01-01T12:00:00Z, in UTC,** unless the test sets it with `clock.set(now:)` before the code under test reads it, so `clock.now` is deterministic and a test can assert exact relative-time text.
- `advance(by:)` moves virtual time forward and fires every timer and sleep that falls due, in order. No real time passes.
- **One virtual time per test.** The test's server container and browser container share it, so a server-side `clock.now` and a browser-side timer agree.
- `screen.settle()` doesn't wait for work that is waiting on the clock.

## Server rendering

- **The server's render instant is serialized with the page.** Until hydration completes, the browser's `Clock` returns that instant from `now`, so the first client render reads the same time the server did. "5 minutes ago" is the same text in the server's HTML and after hydration. After hydration, `now` reads the system time.
- **`TimeZone.local` is the browser's time zone.** The browser runtime sends it in a `k_tz` cookie after the first load, so later server renders format dates in the user's zone. On a first visit, server rendering uses UTC, and the browser re-renders those text nodes after hydration as an expected update, not a hydration mismatch.
- **Formatting uses the metadata locale** (`formatted(as: .dateTime)`), on the server, in the browser and in jobs, never the browser's language setting ([22-metadata.md](22-metadata.md)).

## `TimeOfDay`

`TimeOfDay("03:00")` is a time on a 24-hour clock: a job schedule's time ([30-background-jobs.md](30-background-jobs.md)), and an ordinary value everywhere else, on the server and in the browser.

```k
import k.time.TimeOfDay

struct OpeningHours implements Codable {
    let opens: TimeOfDay
    let closes: TimeOfDay
}

let weekdays = OpeningHours(opens: TimeOfDay("09:30"), closes: TimeOfDay("17:00"))
let summary = "Opens at {weekdays.opens}, {weekdays.opens.hour} hours after midnight"
```

- **A literal is checked when you compile** (`K2404`): `HH:MM`, two digits each, an hour `00` to `23` and a minute `00` to `59` ([20-standard-library.md](20-standard-library.md#literals-and-parsing)).
- **`TimeOfDay.parse(_:)`** accepts exactly what a literal does and returns `nil` for anything else (`"24:00"`, `"7:05"`, `"07:05:00"`), so `K2403`'s quick fix for a non-literal argument switches to it.
- **`hour` and `minute`** are its parts, `0...23` and `0...59`.
- **It is its `HH:MM` text** on both sides: it prints as that text (`"{time}"` is `09:30`), is equal and hashable by it (a `Set` element, a `Map` key), and is encoded as it (`"09:30"`). Decoding accepts only what `parse` accepts, so other text is a `mismatch`, never a value.
- It has no time zone and no date: it names a time on any day's clock. Job schedules read it in UTC.

## Other `k.time` types

- Calendars, time zones and relative-time formatting complete the module ([20-standard-library.md](20-standard-library.md)).

## Why

- **Injection only, with no ambient clock.** Every reading of the time is visible as a dependency, and the layering rules hold with no exception for time ([10-services.md](10-services.md#layering)). An ambient clock would let a plain function read the time unseen. The cost is an `@Inject` line wherever time is read, and a `now: Date` parameter on plain functions, which also makes those functions trivial to test.
- Returning a handle and cancelling it in `onCleanup` reuses the cleanup mechanism the design already has, with no separate lifecycle hook.
- Routing all time through one clock makes time-based behavior (debounce, refresh, relative dates) testable without flaky real-time waits.
- Serializing the render instant and the time zone removes the most common hydration mismatches by construction.
- `TimeOfDay` is a value on both sides, not a schedule-only token, because times of day appear in ordinary app data (opening hours, reminders, quiet hours) as often as in schedules. Representing it as its `HH:MM` text, as `TimeZone` is its name, makes printing, equality, hashing and the wire form the text with no conversion, and it is the form the literal is already written in.
