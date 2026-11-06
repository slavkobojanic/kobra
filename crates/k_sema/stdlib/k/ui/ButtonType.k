/// The `.type(...)` attribute of `button` (`docs/design/05-views.md`).
public enum ButtonType {
    /// Submits the enclosing form. The default inside a `form`.
    case submit
    /// Does nothing by itself; needed on a non-submitting button in a form.
    case button
    /// Resets the enclosing form.
    case reset
}
