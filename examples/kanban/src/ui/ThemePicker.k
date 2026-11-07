import k.ui.Theme

/// Three buttons that choose the look: Light, Dark or High contrast.
///
/// The active variant belongs to the stdlib's `Theme` service, which swaps
/// one class on the document root and remembers the choice, so nothing
/// re-renders and a reload keeps it. Each button says whether it is the
/// current one with `.pressed`.
public component ThemePicker {
    @Inject let theme: Theme

    view {
        nav {
            button { "Light" }
                .pressed(theme.current == .base)
                .on(click: { theme.apply(.base) })

            button { "Dark" }
                .pressed(theme.current == .dark)
                .on(click: { theme.apply(.dark) })

            button { "High contrast" }
                .pressed(theme.current == .contrast)
                .on(click: { theme.apply(.contrast) })
        }
        .label("Theme")
        .class(.picker)
    }

    style {
        .picker {
            display: flex;
            gap: .xs;
        }
    }
}
