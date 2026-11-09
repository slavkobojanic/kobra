/// A rendered view, returned by [`render(_:)`]. It supports every query of
/// [`Element`] over the whole view.
@Native
public class Screen {
    /// The document title from the `metadata` blocks in effect.
    var title: String
    /// The page the router is on ([`k.core.Page`]).
    var currentPage: Page

    /// Waits until no `@Load`, async effect or handler is in flight, apart
    /// from work waiting on the [`TestClock`] or on an open stream.
    func settle() async
    /// Unmounts the view, running every [`k.core.onCleanup`] block.
    func unmount()

    /// The heading whose accessible name is exactly `name`.
    func heading(named name: String) -> Element
    /// The link whose accessible name is exactly `name`.
    func link(named name: String) -> Element
    /// The button whose accessible name is exactly `name`.
    func button(named name: String) -> Element
    /// The element whose text is exactly `text`.
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
