/// The user's system preferences that change behaviour
/// (`docs/design/45-web-platform.md`, "Preferences"). Styling doesn't need
/// it: a theme's variants and style blocks' media queries follow the same
/// preferences in CSS. A stdlib service in `.app` scope, usable on both
/// sides: inject it with `@Inject let preferences: Preferences`, and
/// [`k.test.TestPreferences`] replaces it in tests.
///
/// Each member is reactive, following its media query. During server
/// rendering the dark and reduced-motion members come from the
/// `Sec-CH-Prefers-Color-Scheme` and `Sec-CH-Prefers-Reduced-Motion`
/// client hints where the browser sends them, and every member is `false`
/// otherwise.
@Native
public service Preferences {
    /// `prefers-color-scheme: dark`.
    var prefersDark: Bool
    /// `prefers-reduced-motion: reduce`.
    var prefersReducedMotion: Bool
    /// `prefers-contrast: more`.
    var prefersMoreContrast: Bool
    /// `forced-colors: active` (Windows contrast themes).
    var forcedColors: Bool
}
