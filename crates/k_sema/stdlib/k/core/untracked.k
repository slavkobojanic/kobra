/// Runs `block` without subscribing the enclosing [`Effect`] or [`Derived`]
/// value to what it reads (`docs/design/03-reactivity.md`, "Escape
/// hatches").
///
/// - Parameter block: the reads to make without tracking.
/// - Returns: what `block` returned.
@Native
public func untracked<T>(_ block: () -> T) -> T
