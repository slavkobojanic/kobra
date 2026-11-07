/// An event a component fires, declared with [`Emits`]: `@Emits let
/// changed: Event<Int>` (`docs/design/04-components.md`, "Events").
///
/// A parent listens with `.on(changed: handler)`. Only the declaring
/// component can fire it.
@Native
public struct Event<T> {
    /// Fires the event with `payload`, calling the parent's handler.
    ///
    /// - Parameter payload: the value delivered; for `Event<Void>` it is
    ///   omitted: `closed.emit()`.
    func emit(_ payload: T)
}
