import k.device.Preferences

/// The default [`Preferences`] in `k test`
/// (`docs/design/45-web-platform.md`, "Preferences"): no preference is set
/// until the test sets one. Each member is reactive, like the browser's.
@TestDefault(replaces: Preferences)
@Native
public service TestPreferences implements Preferences {
    /// `false` until [`set(prefersDark:)`].
    var prefersDark: Bool
    /// `false` until [`set(prefersReducedMotion:)`].
    var prefersReducedMotion: Bool
    /// `false` until [`set(prefersMoreContrast:)`].
    var prefersMoreContrast: Bool
    /// `false` until [`set(forcedColors:)`].
    var forcedColors: Bool

    /// Sets [`prefersDark`], as switching the system to dark mode does.
    func set(prefersDark value: Bool)
    /// Sets [`prefersReducedMotion`].
    func set(prefersReducedMotion value: Bool)
    /// Sets [`prefersMoreContrast`].
    func set(prefersMoreContrast value: Bool)
    /// Sets [`forcedColors`].
    func set(forcedColors value: Bool)
}
