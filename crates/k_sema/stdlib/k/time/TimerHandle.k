/// A running timer from `Clock.every` or `Clock.after`. Discarding one
/// returned inside an `@Effect` is a warning.
@Native
public class TimerHandle {
    /// Stops the timer. Idempotent.
    func cancel()
}
