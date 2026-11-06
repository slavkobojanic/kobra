import k.ui.Theme

/// A switch between the light and dark color schemes.
///
/// The checkbox is bound to a custom `Binding` rather than to `@State`,
/// because the active variant belongs to the stdlib's `Theme` service and
/// only its methods may change it. The binding reads `Theme.current` and
/// writes through `Theme.apply(_:)`, which swaps one class on the document
/// root and remembers the choice. Nothing re-renders, and a reload keeps the
/// scheme.
component ThemeToggle {
    @Inject let theme: Theme

    func useDarkVariant(isOn: Bool) {
        theme.apply(if isOn { .dark } else { .base })
    }

    view {
        checkbox(isChecked: Binding(get: { theme.current == .dark }, set: useDarkVariant))
            .label("Dark mode")
    }
}
