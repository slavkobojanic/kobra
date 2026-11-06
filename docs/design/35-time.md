# Timers and the Clock

## Decision

`k.time` provides **`Timer`** for delays and repetition, and a **clock** that every reading of the current time goes through, so tests control time with a virtual clock and never wait in real time.

```k
@Effect(on: [placeId])
func refreshPeriodically() {
    let timer = Timer.every(10.minutes) { reload() }

    onCleanup { timer.cancel() }
}
```

## Timer

```k
let timer = Timer.every(10.minutes) { reload() }     // repeats; first fire after one interval
let once = Timer.after(2.seconds) { dismiss() }      // fires once
timer.cancel()                                        // idempotent

await Timer.sleep(300ms)                              // suspends the current async function
```

- `Timer.every` and `Timer.after` return a `TimerHandle` with `cancel()`.
- **Timers belong to their owner.** A timer started inside an `@Effect` should be cancelled in `onCleanup`. The compiler warns when a `TimerHandle` returned inside an effect is discarded, because the timer would outlive the component.
- **`Timer.sleep` is cancellable.** When the async effect or `@Load` that is sleeping is cancelled, the sleep ends and the function never resumes. Debouncing is therefore an async effect that sleeps first, and that handles its own errors, as every effect must ([03-reactivity.md](03-reactivity.md#effects-handle-their-own-errors)):

  ```k
  @Effect
  func search() async {
      let text = query

      await Timer.sleep(300ms)              // a new keystroke cancels this run

      do {
          results = .loaded(try await places.search(text))
      } catch let error {
          results = .failed(error)
      }
  }
  ```

- **Effects don't run during server rendering,** so neither do timers started in them. Server-side work on a schedule is a background job ([30-background-jobs.md](30-background-jobs.md#schedules)).

## The clock

`Date.now`, `2.hours.ago`, relative-time formatting and every `Timer` call read the clock. The only production implementation is `SystemClock`.

### Open question: an ambient clock or an injected one

**Not decided.** The decided docs already read the time from anywhere (`Date.now + 3.days` in [07-themes-and-units.md](07-themes-and-units.md)), including plain functions and structs, which inject nothing ([10-services.md](10-services.md#layering)). So the clock can't be an ordinary injected interface without changing that. The options:

- **An ambient context (recommended).** `Clock` is declared as the stdlib's single ambient context, explicitly not a dependency-injection binding. `Date.now`, `.ago` and `Timer` read it from anywhere. The test runner installs a virtual clock per test, which tests control through `@Inject let clock: TestClock`. `use … for Clock` is not allowed, so `use` keeps one meaning. Existing examples stay as they are.
- **Strict injection.** Code that needs the time writes `@Inject let clock: Clock` and calls `clock.now` and `clock.every(…)`. `Date.now` and the ambient `Timer` are removed, and `.ago` takes a clock. This is more explicit, and the layering rules hold without an exception, but it is noisier, keeps time out of plain functions and structs, and changes decided examples in [07-themes-and-units.md](07-themes-and-units.md).

Either way, tests reach the virtual clock with `@Inject let clock: TestClock` and never write a `use` line for it ([21-testing.md](21-testing.md#test-doubles-and-defaults)).

### Testing with `TestClock`

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

- **`TestClock` is the clock in every `k test`.** Virtual time starts at a fixed instant, so `Date.now` is deterministic.
- `advance(by:)` moves virtual time forward and fires every timer and sleep that falls due, in order. No real time passes.
- `screen.settle()` doesn't wait for work that is waiting on the clock.

## Server rendering

- **The server's render instant is serialized with the page,** and the first client render reads `Date.now` from it. "5 minutes ago" is the same text in the server's HTML and after hydration. Later renders read the clock normally.
- **`TimeZone.local` is the browser's time zone.** The browser runtime sends it in a `k_tz` cookie after the first load, so later server renders format dates in the user's zone. On a first visit, server rendering uses UTC, and the browser re-renders those text nodes after hydration as an expected update, not a hydration mismatch.
- **Formatting uses the metadata locale** (`formatted(as: .dateTime)`), on the server, in the browser and in jobs, never the browser's language setting ([22-metadata.md](22-metadata.md)).

## Other `k.time` types

- **`TimeOfDay("03:00")`** is a time of day, a literal validated at compile time, used by job schedules.
- Calendars, time zones and relative-time formatting complete the module ([20-standard-library.md](20-standard-library.md)).

## Why

- Returning a handle and cancelling it in `onCleanup` reuses the cleanup mechanism the design already has, with no separate lifecycle hook.
- Routing all time through one clock makes time-based behavior (debounce, refresh, relative dates) testable without flaky real-time waits.
- Serializing the render instant and the time zone removes the most common hydration mismatches by construction.
