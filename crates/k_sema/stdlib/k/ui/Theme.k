/// The app's active theme variant (`docs/design/07-themes-and-units.md`,
/// "Switching variants at runtime"). Client-only; inject it with
/// `@Inject let theme: Theme`.
@ClientOnly
@Native
public service Theme {
    /// The active variant ([`k.core.ThemeVariant`]). Reactive.
    var current: ThemeVariant

    /// Makes `variant` active by swapping one class on the document root,
    /// and remembers the choice in the `k_theme` cookie.
    func apply(_ variant: ThemeVariant)

    /// Forgets the explicit choice, so variants that `match` a system
    /// preference apply again.
    func followSystem()
}
