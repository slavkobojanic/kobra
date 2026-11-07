/// A side effect that re-runs when the reactive values it read change
/// (`docs/design/03-reactivity.md`). `@Effect(on: [userId])` lists the
/// dependencies explicitly, and `@Effect(on: [])` runs once after mount. An
/// effect can't be `throws`.
@Annotation
public struct Effect {
    /// The explicit dependencies; `nil` tracks what the body reads, and
    /// `[]` runs the effect once after mount.
    let on: List<Dependency>? = nil
}
