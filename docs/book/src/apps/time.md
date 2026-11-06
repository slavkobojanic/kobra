# Time and timers

`k.time` provides **`Timer`** for delays and repetition, and a **clock** that every reading of the
current time goes through. Tests control time with a virtual clock and never wait in real time.

```k
@Effect(on: [placeId])
func refreshPeriodically() {
    let timer = Timer.every(10.minutes) { reload() }

    onCleanup { timer.cancel() }
}
```

## `Timer`

```k
let timer = Timer.every(10.minutes) { reload() }     // repeats; first fire after one interval
let once = Timer.after(2.seconds) { dismiss() }      // fires once
timer.cancel()                                        // idempotent

await Timer.sleep(300ms)                              // suspends the current async function
```

- `Timer.every` and `Timer.after` return a `TimerHandle` with `cancel()`.
- **Timers belong to their owner.** A timer started inside an `@Effect` should be cancelled in
  `onCleanup`. The compiler warns when a `TimerHandle` returned inside an effect is discarded, because the
  timer would outlive the component.
- **`Timer.sleep` is cancellable.** When the effect or `@Load` that is sleeping is cancelled, the sleep
  ends and the function never resumes. Debouncing is therefore an async effect that sleeps first:

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

- **Effects don't run during server rendering,** so neither do timers started in them. Server-side work
  on a schedule is a [background job](jobs.md#schedules).

## The clock

`Date.now`, `2.hours.ago`, relative-time formatting and every `Timer` call read the clock. The only
production implementation is `SystemClock`.

> **Open question.** How code reaches the clock isn't decided. The recommended design makes `Clock` the
> standard library's single *ambient* context: `Date.now`, `.ago` and `Timer` read it from anywhere,
> including plain functions and structs, as this book's examples do. The alternative is strict
> injection, with `@Inject let clock: Clock` and `clock.now`. Either way, tests reach the virtual clock
> with `@Inject let clock: TestClock` and never write a `use` line for it. See
> `docs/design/35-time.md`.

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

- **`TestClock` is the clock in every `k test`.** Virtual time starts at a fixed instant, so `Date.now` is
  deterministic.
- `advance(by:)` moves virtual time forward and fires every timer and sleep that falls due, in order. No
  real time passes.
- `screen.settle()` doesn't wait for work that is waiting on the clock.

## Server rendering

- **The server's render instant is serialized with the page,** and the first client render reads
  `Date.now` from it. "5 minutes ago" is the same text in the server's HTML and after hydration. Later
  renders read the clock normally.
- **`TimeZone.local` is the browser's time zone.** The browser runtime sends it in a `k_tz` cookie after
  the first load, so later server renders format dates in the user's zone. On a first visit, server
  rendering uses UTC, and the browser re-renders those text nodes after hydration as an expected update,
  not a hydration mismatch.
- **Formatting uses the metadata locale** (`formatted(as: .dateTime)`), on the server, in the browser and
  in jobs, never the browser's language setting.

## Other `k.time` types

- **`TimeOfDay("03:00")`** is a time of day, a literal validated at compile time, used by job schedules.
- Calendars, time zones and relative-time formatting complete the module.
