/// A running timer from [`Clock.every`] or [`Clock.after`]. Discarding one
/// returned inside an [`k.core.Effect`] is a warning.
@Native
public class TimerHandle {
    /// Stops the timer; cancelling a stopped timer does nothing.
    func cancel()
}
