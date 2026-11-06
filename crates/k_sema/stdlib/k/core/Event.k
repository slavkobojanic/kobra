/// An event a component fires, declared with `@Emits let changed:
/// Event<Int>` (`docs/design/04-components.md`, "Events").
///
/// A parent listens with `.on(changed: handler)`. Only the declaring
/// component can fire it.
@Native
public struct Event<T> {
    /// Fires the event with `payload`. For `Event<Void>` the payload is
    /// omitted: `closed.emit()`.
    func emit(_ payload: T)
}
