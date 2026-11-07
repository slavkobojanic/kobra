/// Refers to a class from global CSS, escaping scoping:
/// `.class(global("btn-primary"))` (`docs/design/06-styles.md`, "Scoping").
///
/// - Parameter name: the class name as written in the CSS, without the dot.
/// - Returns: [`Classes`] holding that one class.
@Native
public func global(_ name: String) -> Classes
