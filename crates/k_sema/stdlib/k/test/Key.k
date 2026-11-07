/// A key for [`Element.press`].
public enum Key {
    /// The Enter key.
    case enter
    /// The Escape key.
    case escape
    /// The Tab key.
    case tab
    /// The space bar.
    case space
    /// The up arrow key.
    case arrowUp
    /// The down arrow key.
    case arrowDown
    /// The left arrow key.
    case arrowLeft
    /// The right arrow key.
    case arrowRight
    /// The key that types `text` ("n", "?"), for keyboard shortcuts.
    case character(String)
}
