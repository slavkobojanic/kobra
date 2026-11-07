// `theme.apply(.dark)` and `theme.current` go through the injected client-only `Theme` service;
// the generated `ThemeVariant` cases are compared by identity (07-themes-and-units.md).
import k.ui.Theme

theme Tokens {
    color surface: Color = #FFFFFF

    variant dark {
        surface = #111827
    }
}

component ThemeToggle {
    @Inject let theme: Theme

    func toggle() {
        theme.apply(if theme.current == .dark { .base } else { .dark })
    }

    view {
        button { "Dark mode" }
            .pressed(theme.current == .dark)
            .on(click: toggle)
    }
}
