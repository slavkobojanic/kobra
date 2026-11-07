import k.time.Clock
import k.time.TimerHandle

/// Virtual time for tests, the default [`Clock`] in `k test`
/// (`docs/design/35-time.md`, "Testing with TestClock"). It starts at
/// 2026-01-01T12:00:00Z (UTC) unless the test sets it, and is shared by the
/// test's server and browser containers.
@TestDefault(replaces: Clock)
@Native
public service TestClock implements Clock {
    /// The current virtual instant.
    var now: Date

    /// Runs `action` every `interval` of virtual time.
    func every(_ interval: Duration, _ action: () -> Void) -> TimerHandle
    /// Runs `action` once after `delay` of virtual time.
    func after(_ delay: Duration, _ action: () -> Void) -> TimerHandle
    /// Suspends until `duration` of virtual time has passed.
    func sleep(_ duration: Duration) async

    /// Moves virtual time forward by `duration`, firing every timer and
    /// sleep that falls due, in order.
    func advance(by duration: Duration)
    /// Sets the virtual instant to `now`, before the code under test reads
    /// it.
    func set(now: Date)
}
