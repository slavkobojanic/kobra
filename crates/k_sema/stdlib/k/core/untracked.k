/// Runs `block` without subscribing the enclosing effect or `@Derived` value
/// to what it reads, and returns its value
/// (`docs/design/03-reactivity.md`, "Escape hatches").
@Native
public func untracked<T>(_ block: () -> T) -> T
