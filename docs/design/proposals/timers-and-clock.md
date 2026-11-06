# Proposal: Timers and the Clock

> **Status: proposal.** Not yet accepted into the design. First used by `examples/weather`.

## Problem

[07-themes-and-units.md](../07-themes-and-units.md) shows `Timer.every(30.seconds) { refresh() }` but doesn't say how a timer is stopped, how code waits for a duration, or how tests control time without real waiting.

## Proposal

### Timer

```k
let timer = Timer.every(10.minutes) { reload() }     // repeats; first fire after one interval
let once = Timer.after(2.seconds) { dismiss() }      // fires once
timer.cancel()                                        // idempotent

await Timer.sleep(300ms)                              // suspends the current async function
```

- `Timer.every` and `Timer.after` return a `TimerHandle` with `cancel()`.
- **Timers belong to their owner.** A timer started inside an `@Effect` should be cancelled in `onCleanup`. The compiler warns when a `TimerHandle` returned inside an effect is discarded, because the timer would outlive the component:

  ```k
  @Effect(on: [placeId])
  func refreshPeriodically() {
      let timer = Timer.every(refreshInterval) { reload() }

      onCleanup { timer.cancel() }
  }
  ```

- **`Timer.sleep` is cancellable.** When the async effect or `@Load` that is sleeping is cancelled, the sleep ends and the function never resumes. Debouncing is therefore just an async effect that sleeps first:

  ```k
  @Effect
  func search() async {
      let text = query

      await Timer.sleep(300ms)              // a new keystroke cancels this run

      results = .loaded(try await places.search(text))
  }
  ```

- **Effects don't run during server rendering**, so neither do timers started in them. A server-side service that needs a schedule uses `k.server` background jobs.

### Clock

- `Clock` is a stdlib interface in `k.time` with `now: Date` and the scheduling primitives behind `Timer`. `Date.now`, `2.hours.ago` and every `Timer` call read the injected clock.
- The only production implementation, `SystemClock`, is injected automatically.
- **Tests control time** with `TestClock` from `k.test`:

  ```k
  test CityWeather {
      use TestClock for Clock

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

  - `advance(by:)` moves virtual time forward and fires every timer and sleep that falls due, in order. No real time passes.
  - Virtual time starts at a fixed instant, so `Date.now` is deterministic.
  - `TestClock` is installed by default in `k test`. Writing `use TestClock for Clock` documents that a test depends on time.

## Why

- Returning a handle and cancelling it in `onCleanup` reuses the cleanup mechanism the design already has, with no separate lifecycle hook.
- Routing all time through one injectable interface makes time-based behaviour (debounce, refresh, relative dates) testable without flaky real-time waits, using the existing `use X for Y` mechanism.
