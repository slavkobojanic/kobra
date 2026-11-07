/// A handle to the elements a query matched. A singular query returns a
/// handle even when nothing matches; an action on a handle that matches
/// nothing, or several elements, fails the test.
///
/// Every handle supports the queries of [`Screen`], scoped to its subtree.
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
    /// The current text of a field.
    var value: String
    /// The element's text content.
    var text: String
    /// The level of a heading, `1...6`.
    var level: Int

    /// Clicks the element and flushes synchronous updates.
    func click()
    /// Replaces a field's value and fires input events.
    func fill(_ text: String)
    /// Presses `key` ([`Key`]); [`Key.enter`] in a form field submits the
    /// form.
    func press(_ key: Key)
    /// Drags the element onto `target` with the mouse: presses it, moves
    /// onto `target` and releases there (`docs/design/41-drag-and-drop.md`,
    /// "Tests").
    func drag(to target: Element)

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
    /// Returns the only list.
    func list() -> Element
    /// The list whose accessible name is exactly `name`.
    func list(named name: String) -> Element
    /// The only `.role(.alert)` live region.
    func alert() -> Element
    /// The only `.role(.status)` live region.
    func status() -> Element
    /// Returns every list item, in document order.
    func items() -> List<Element>
    /// Returns every button, in document order.
    func buttons() -> List<Element>
    /// Returns every heading, in document order.
    func headings() -> List<Element>
    /// Returns every link, in document order.
    func links() -> List<Element>
}
