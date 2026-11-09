import k.device.PageVisibility

/// The default [`PageVisibility`] in `k test`
/// (`docs/design/45-web-platform.md`, "Page visibility"): the page is
/// visible until [`hide()`].
@TestDefault(replaces: PageVisibility)
@Native
public service TestPageVisibility implements PageVisibility {
    /// `true` until [`hide()`]. Reactive, like the browser's.
    var isVisible: Bool

    /// Hides the page, as switching to another tab does.
    func hide()
    /// Shows the page again.
    func show()
}
