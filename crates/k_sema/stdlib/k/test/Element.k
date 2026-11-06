/// A handle to the elements a query matched. A singular query returns a
/// handle even when nothing matches; an action on a handle that matches
/// nothing, or several elements, fails the test.
///
/// Every handle supports the queries of `Screen`, scoped to its subtree.
@Native
public class Element {
    /// `true` if exactly one element matched.
    var exists: Bool
    /// `true` if it exists and is visible.
    var isVisible: Bool
    /// `true` for a checked checkbox.
    var isChecked: Bool
    /// `true` for a pressed button.
    var isPressed: Bool
    /// `true` for a disabled control.
    var isDisabled: Bool
    /// A field's current text.
    var value: String
    /// The element's text content.
    var text: String
    /// A heading's level.
    var level: Int

    /// Clicks the element and flushes synchronous updates.
    func click()
    /// Replaces a field's value and fires input events.
    func fill(_ text: String)
    /// Presses a key; Enter in a form field submits the form.
    func press(_ key: Key)

    /// The heading whose accessible name is exactly `name`.
    func heading(named name: String) -> Element
    /// The link whose accessible name is exactly `name`.
    func link(named name: String) -> Element
    /// The button whose accessible name is exactly `name`.
    func button(named name: String) -> Element
    /// The element whose text is exactly `text`, after whitespace
    /// normalisation.
    func text(_ text: String) -> Element
    /// An element whose text contains `text`.
    func text(containing text: String) -> Element
    /// The text field labelled exactly `label`.
    func textField(labeled label: String) -> Element
    /// The password field labelled exactly `label`.
    func passwordField(labeled label: String) -> Element
    /// The text area labelled exactly `label`.
    func textArea(labeled label: String) -> Element
    /// The number field labelled exactly `label`.
    func numberField(labeled label: String) -> Element
    /// The dropdown labelled exactly `label`.
    func dropdown(labeled label: String) -> Element
    /// The checkbox labelled exactly `label`.
    func checkbox(labeled label: String) -> Element
    /// A list item whose text contains `text`.
    func item(containing text: String) -> Element
    /// The `nav` landmark whose accessible name is exactly `name`.
    func navigation(named name: String) -> Element
    /// The only list.
    func list() -> Element
    /// The only `.role(.alert)` live region.
    func alert() -> Element
    /// The only `.role(.status)` live region.
    func status() -> Element
    /// Every list item.
    func items() -> List<Element>
    /// Every button.
    func buttons() -> List<Element>
    /// Every heading.
    func headings() -> List<Element>
    /// Every link.
    func links() -> List<Element>
}
