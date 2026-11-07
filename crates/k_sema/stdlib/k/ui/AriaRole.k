/// The roles `.role(...)` accepts: only roles no semantic element expresses
/// (`docs/design/05-views.md`, "Attributes are per element").
public enum AriaRole {
    /// An important, time-sensitive message (a live region).
    case alert
    /// An advisory status message (a live region).
    case status
    /// A group of related controls.
    case group
    /// A toolbar: a row of controls.
    case toolbar
    /// A region of the page; use `section` when it has a heading.
    case region
    /// A navigation landmark; always a compile error, with the hint "use
    /// `nav`".
    case navigation
}
