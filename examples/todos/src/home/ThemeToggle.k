import appearance.AppearanceService

/// A switch between the light and dark color schemes.
///
/// The checkbox is bound to a custom `Binding` rather than to `@State`,
/// because the choice belongs to `AppearanceService` and only its methods may
/// change it. Reading goes through `get`; writing calls `choose(scheme:)`, so
/// the choice is saved as well as shown.
component ThemeToggle {
    @Inject let appearance: AppearanceService

    func useDarkScheme(isOn: Bool) {
        appearance.choose(scheme: if isOn { .dark } else { .light })
    }

    view {
        checkbox(isChecked: Binding(get: { appearance.scheme == .dark }, set: useDarkScheme))
            .label("Dark mode")
    }
}
