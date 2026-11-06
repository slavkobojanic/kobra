/// The current time, timers and sleeps (`docs/design/35-time.md`). Inject it
/// with `@Inject let clock: Clock`; there is no ambient clock. A service,
/// usable on both sides and in every scope; `TestClock` replaces it in
/// tests.
@Native
public service Clock {
    /// The current instant. Not reactive: reading it subscribes to nothing.
    var now: Date

    /// Runs `action` every `interval`; the first run is after one interval.
    func every(_ interval: Duration, _ action: () -> Void) -> TimerHandle
    /// Runs `action` once, after `delay`.
    func after(_ delay: Duration, _ action: () -> Void) -> TimerHandle
    /// Suspends the current async function for `duration`; cancellable.
    func sleep(_ duration: Duration) async
}
