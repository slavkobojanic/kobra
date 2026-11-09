/// A running callback subscription, such as a handler listening for
/// messages from the app's other tabs
/// (`docs/design/45-web-platform.md`, "Ownership and lifetime"). Discarding
/// one returned inside an [`k.core.Effect`] is a warning, as for a
/// [`k.time.TimerHandle`].
@Native
public class Subscription {
    /// Stops the callbacks; cancelling a cancelled subscription does
    /// nothing.
    func cancel()
}
