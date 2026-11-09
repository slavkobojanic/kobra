/// Design tokens for Memos, with a dark variant that follows the operating
/// system's color scheme.
public theme MemosTheme {
    color primary: Color = #B91C1C
    color surface: Color = #FFFFFF
    color text: Color = #1C1917
    color muted: Color = #78716C
    color border: Color = #E7E5E4

    space xs: Spacing = 4px
    space sm: Spacing = 8px
    space md: Spacing = 16px
    space lg: Spacing = 24px

    radius md: Radius = 8px

    variant dark matches .prefersDark {
        surface = #1C1917
        text = #F5F5F4
        muted = #A8A29E
        border = #44403C
    }
}
