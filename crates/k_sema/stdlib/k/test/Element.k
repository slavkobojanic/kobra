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
    /// `true` for a `video` or `audio` element with something to play: a
    /// stream with a live track, a file or a URL.
    var isPlaying: Bool
    /// `true` for a `video` or `audio` element that plays no sound.
    var isMuted: Bool
    /// `true` when the element has the focus
    /// (`docs/design/53-element-references.md`, "Tests").
    var isFocused: Bool
    /// `true` once code scrolled the element into view
    /// ([`k.ui.ElementRef.scrollIntoView(alignment:animated:)`]). The test
    /// DOM has no layout, so nothing else scrolls.
    var wasScrolledIntoView: Bool

    /// Clicks the element and flushes synchronous updates.
    func click()
    /// Replaces a field's value and fires input events.
    func fill(_ text: String)
    /// Presses `key` ([`Key`]); [`Key.enter`] in a form field submits the
    /// form.
    func press(_ key: Key)
    /// Moves the focus away from the element, firing `blur` and
    /// `focusout`, as leaving a form field does (it starts the field's
    /// `@Check` rules, `docs/design/15-forms-and-validation.md`).
    func blur()
    /// Drags the element onto `target` with the mouse: presses it, moves
    /// onto `target` and releases there (`docs/design/41-drag-and-drop.md`,
    /// "Tests").
    func drag(to target: Element)
    /// Makes the element appear in the viewport, firing its `appeared`
    /// event (`docs/design/45-web-platform.md`, "Element events"). The
    /// test DOM has no layout, so nothing appears unless a test says so.
    func appear()
    /// Makes the element leave the viewport, firing its `disappeared`
    /// event.
    func disappear()
    /// Gives the element's content box a new size, firing its `resized`
    /// event.
    func resize(width: Length, height: Length)

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
    /// The `video` element labelled exactly `label` (`.label(…)`).
    func video(labeled label: String) -> Element
    /// The `audio` element labelled exactly `label` (`.label(…)`).
    func audio(labeled label: String) -> Element
    /// Returns every list item, in document order.
    func items() -> List<Element>
    /// Returns every button, in document order.
    func buttons() -> List<Element>
    /// Returns every heading, in document order.
    func headings() -> List<Element>
    /// Returns every link, in document order.
    func links() -> List<Element>
}
