/// Registers `block` to run before the enclosing [`Effect`] re-runs and
/// when its component unmounts (`docs/design/03-reactivity.md`).
///
/// - Parameter block: the cleanup, such as cancelling a timer the effect
///   started.
@Native
public func onCleanup(_ block: () -> Void)
