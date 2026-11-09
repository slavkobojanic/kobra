# Time and timers

**The current time, timers and sleeps come only from the `Clock` service** in `k.time`. Code that needs
the time writes `@Inject let clock: Clock`; there is no ambient way to read it. Tests get a virtual clock
by default, so they control time and never wait in real time.

```k
// examples/shop/shop/src/pages/Receipt.k
/// While the payment is being confirmed, checks again every few seconds,
/// so the page turns to "Payment received" without a manual refresh. The
/// effect re-runs when `order.status` changes, which cancels the timer.
@Effect
func refreshWhileConfirming() {
    guard order.status == .awaitingPayment else {
        return
    }

    let timer = clock.every(3.seconds) { reload() }

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
~import k.time.Clock
~
~service Timers {
~    @Inject let clock: Clock
~
~    func reload() {}
~
~    func dismiss() {}
~
~    func example() async {
let timer = clock.every(10.minutes) { reload() }     // repeats; first fire after one interval
let once = clock.after(2.seconds) { dismiss() }      // fires once
timer.cancel()                                        // idempotent

await clock.sleep(300ms)                              // suspends the current async function
~    }
~}
```

- **It is a service, so it goes where services go:** components, pages, layouts, services, endpoint pages
  and jobs may inject it ([Services](services.md#layering)). It holds no request state, so services in
  every scope may inject it, `.server` included.
- **Plain functions and structs don't read the time.** They inject nothing, so code that needs the
  current instant takes it as a parameter, usually named `now`:

  ```k
  // examples/shop/shop/src/payments/StripeSignature.k
  func isFresh(now: Date, tolerance: Duration) -> Bool {
      return signedAt >= now - tolerance and signedAt <= now + tolerance
  }
  ```

  ```k
  // examples/shop/shop/src/payments/StripeWebhookVerifier.k
  guard signature.isFresh(now: clock.now, tolerance: config.tolerance) else {
      Log.warning("Stripe webhook signed at {signature.signedAt} is outside the tolerance")

      throw WebhookError.staleTimestamp
  }
  ```

- **There is no `Date.now`, `.ago` or `.fromNow`.** "Ten minutes ago" is `clock.now - 10.minutes`, and
  "in three days" is `clock.now + 3.days`. Durations themselves need no clock: `10.minutes` is a
  `Duration` ([Units](../language/units.md)).
- `every` and `after` return a `TimerHandle` with `cancel()`.
- **Timers belong to their owner.** A timer a component starts, in an effect, a handler or a lambda,
  stops when the component is disposed, so a page you leave stops its timers. A timer started inside an
  `@Effect` should still be cancelled in `onCleanup`, so that a re-run of the effect doesn't add a
  second timer to the first. The compiler warns when a `TimerHandle` returned inside an effect is
  discarded.
- **`clock.sleep` is cancellable.** When the async effect or `@Load` that is sleeping is cancelled, the
  sleep ends and the function never resumes. Debouncing is therefore an async effect that sleeps first,
  and that handles its own errors, as every effect must:

  ```k
  ~import k.time.Clock
  ~
  ~enum SearchError implements Error, Codable {
  ~    case offline
  ~}
  ~
  ~struct Place implements Codable {
  ~    let name: String
  ~}
  ~
  ~service Places {
  ~    func search(_ text: String) async throws SearchError -> List<Place> {
  ~        return []
  ~    }
  ~}
  ~
  ~component Search {
  ~    @Inject let clock: Clock
  ~    @Inject let places: Places
  ~    @State var query: String = ""
  ~    @State var results: LoadState<List<Place>> = .idle
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
  ~    view {
  ~        text { query }
  ~    }
  ~}
  ```

- **Effects don't run during server rendering,** so neither do timers started in them. Server-side work
  on a schedule is a [background job](jobs.md#schedules).

## Relative times

Relative formatting takes the reference instant explicitly, so it reads the clock where the clock is
injected:

```k
// examples/notes/src/notes/NoteCard.k
text { "Edited {note.updatedAt.formatted(as: .relative, from: clock.now)}" }
```

- **`date.formatted(as: .relative, from: reference)`** describes `date` relative to `reference`:
  "5 minutes ago", "in 3 days". The `from:` argument is required.
- The other styles (`.date`, `.time`, `.dateTime`) need no reference instant.

## Testing with `TestClock`

```k
// examples/weather/src/pages/CityWeather.test.k
test "the forecast refreshes every ten minutes" {
    let screen = render(CityWeather(placeId: Place.berlin.id))
    await screen.settle()

    expect weather.forecastCalls == 1

    clock.advance(by: 9.minutes)
    await screen.settle()

    expect weather.forecastCalls == 1

    clock.advance(by: 1.minute)
    await screen.settle()

    expect weather.forecastCalls == 2
    // ...
}
```

- **`TestClock` is the default for `Clock` in every `k test`** ([Testing](testing.md#test-doubles-and-defaults)).
  A test reaches it with `@Inject let clock: TestClock` and never writes a `use` line for it.
- Virtual time starts at a fixed instant, so `clock.now` is deterministic.
- `advance(by:)` moves virtual time forward and fires every timer and sleep that falls due, in order. No
  real time passes.
- **One virtual time per test.** The test's server container and browser container share it, so a
  server-side `clock.now` and a browser-side timer agree.
- `screen.settle()` doesn't wait for work that is waiting on the clock.

## Server rendering

- **The server's render instant is serialized with the page.** Until hydration completes, the browser's
  `Clock` returns that instant from `now`, so the first client render reads the same time the server
  did. "5 minutes ago" is the same text in the server's HTML and after hydration. After hydration, `now`
  reads the system time.
- **`TimeZone.local` is the browser's time zone.** The browser runtime sends it in a `k_tz` cookie after
  the first load, so later server renders format dates in the user's zone. On a first visit, server
  rendering uses UTC, and the browser re-renders those text nodes after hydration as an expected update,
  not a hydration mismatch.
- **Formatting uses the metadata locale** (`formatted(as: .dateTime)`), on the server, in the browser and
  in jobs, never the browser's language setting.

## Times of day

`TimeOfDay("03:00")` is a time on a 24-hour clock. Job schedules use it, and it's an ordinary value
anywhere else, on the server and in the browser:

```k
import k.time.TimeOfDay

struct OpeningHours implements Codable {
    let opens: TimeOfDay
    let closes: TimeOfDay
}

let weekdays = OpeningHours(opens: TimeOfDay("09:30"), closes: TimeOfDay("17:00"))
let summary = "Opens at {weekdays.opens}, {weekdays.opens.hour} hours after midnight"
```

- **A literal is checked when you compile:** `HH:MM`, two digits each, so `TimeOfDay("24:00")` and
  `TimeOfDay("7:05")` don't compile (`K2404`).
- **`TimeOfDay.parse(text)`** reads one at run time and returns `nil` for text a literal couldn't be.
- **`hour` and `minute`** are its parts.
- **It is its `HH:MM` text:** `"{time}"` prints `09:30`, two equal times are `==`, it can be a set
  element or a map key, and it's encoded as `"09:30"`. Decoding anything else is a `mismatch`.
- It has no date and no time zone. Job schedules read it in UTC.

## Other `k.time` types

- **`TimeZone`** is `.utc`, `.local` (the browser's zone) or `TimeZone.parse(name)` for an IANA name
  such as `"Europe/Berlin"`, and **`Weekday`** names the days `.weekly(on:at:)` schedules take.
  Calendars aren't part of `k.time` yet ([Standard library](../reference/stdlib.md)).

## Why injection only

Every reading of the time is visible as a dependency, and the layering rules hold with no exception for
time. An ambient clock would let a plain function read the time unseen. The cost is an `@Inject` line
wherever time is read, and a `now: Date` parameter on plain functions, which also makes those functions
trivial to test.
