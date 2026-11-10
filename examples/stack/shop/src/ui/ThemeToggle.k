import k.ui.Theme

/// A switch between the light and dark color schemes.
///
/// `Theme.apply(_:)` swaps one class on the document root and remembers the
/// choice in a cookie named after the app's port (`k_theme_8081`), so the
/// shop and paybox on one `localhost` keep their own choice.
public component ThemeToggle {
    @Inject let theme: Theme

    func useDarkVariant(isOn: Bool) {
        theme.apply(if isOn { .dark } else { .base })
    }

    view {
        checkbox(isChecked: Binding(get: { theme.current == .dark }, set: useDarkVariant))
            .label("Dark mode")
    }
}
