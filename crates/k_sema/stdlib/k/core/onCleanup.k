/// Registers `block` to run before the enclosing effect re-runs and when its
/// component unmounts (`docs/design/03-reactivity.md`).
@Native
public func onCleanup(_ block: () -> Void)
