/// The app's design tokens: colors, spacing and fonts, with a dark variant
/// that follows the system's dark mode.
public theme AppTheme {
    color primary: Color = #2563EB
    color onPrimary: Color = #FFFFFF
    color text: Color = #1C1917
    color muted: Color = #78716C
    color background: Color = #FAFAF9
    color surface: Color = #FFFFFF
    color border: Color = #E7E5E4

    space sm: Spacing = 8px
    space md: Spacing = 16px
    space lg: Spacing = 24px

    radius md: Radius = 8px

    font body: Font = Font(family: "system-ui", size: 16px)

    variant dark matches .prefersDark {
        primary = #93C5FD
        onPrimary = #1C1917
        text = #F5F5F4
        muted = #A8A29E
        background = #1C1917
        surface = #292524
        border = #44403C
    }
}
