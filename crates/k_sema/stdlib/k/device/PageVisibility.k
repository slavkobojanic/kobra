/// Whether the page is on screen (`docs/design/45-web-platform.md`, "Page
/// visibility"). A stdlib service in `.app` scope, usable on both sides:
/// inject it with `@Inject let visibility: PageVisibility`, and
/// [`k.test.TestPageVisibility`] replaces it in tests.
@Native
public service PageVisibility {
    /// `false` while the tab is in the background, minimized or the screen
    /// is locked (`document.visibilityState`). Reactive: an effect that
    /// reads it starts and stops work with the page's visibility. `true`
    /// during server rendering.
    var isVisible: Bool
}
