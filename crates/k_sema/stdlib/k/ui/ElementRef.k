/// Names one element of a component's own view, so its handlers can move
/// the focus to it or scroll to it (`docs/design/53-element-references.md`):
/// a field `let titleField = ElementRef()`, the attribute
/// `textField(value: bind title).ref(titleField)`, and
/// `titleField.focus()` in a handler.
///
/// An operation runs once the view has caught up with the state changed
/// before it: at the end of the handler or effect that called it. It does
/// nothing while the element isn't shown. Only handlers, effects, `init()`
/// and the functions they call use one; a view, a `@Derived` or another
/// field initializer can't (K3116).
@Native
public class ElementRef {
    /// Creates a reference naming no element yet; `.ref(…)` gives it one.
    @Native
    init() {}

    /// Moves the focus to the element. An element that can't take the
    /// focus by itself (a heading, a section) gets `tabindex="-1"`, so it
    /// takes it from code without becoming a Tab stop. `selectingText`
    /// also selects a text field's or text area's text.
    func focus(selectingText: Bool = false)
    /// Takes the focus away from the element when it has it.
    func blur()
    /// Scrolls the page, and every scrolling box around the element, until
    /// it is in view. `animated` scrolls smoothly unless the user prefers
    /// reduced motion.
    func scrollIntoView(alignment: ScrollAlignment = .nearest, animated: Bool = true)
}
