// The generated `ThemeVariant` has `.base` and one case per variant; tokens of the
// app theme resolve in any module without an import. `.light` is not a variant
// (K2006).
import k.ui.Theme

public component ThemeToggle {
    @Inject let theme: Theme

    func toggle() {
        theme.apply(if theme.current == .dark { .base } else { .dark })
        //                                       ^^^^ resolves: member ThemeVariant.base
        //                                                      ^^^^ resolves: member ThemeVariant.dark
    }

    func contrast() {
        theme.apply(.highContrast)
    }

    func light() {
        theme.apply(.light)
    }

    view {
        button { "Dark mode" }
            .pressed(theme.current == .dark)
            .on(click: toggle)
            .class(.toggle)
    }

    style {
        .toggle {
            backgroundColor: .surface;
            //                ^^^^^^^ resolves: token Color.surface
            color: .text;
            padding: .md;
        }
    }
}
