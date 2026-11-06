/// A class from global CSS, escaping scoping: `.class(global("btn-primary"))`
/// (`docs/design/06-styles.md`, "Scoping").
@Native
public func global(_ name: String) -> Classes
