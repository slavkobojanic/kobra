/// Design tokens for the stack's shop, with a dark variant that follows the
/// operating system's color scheme until the switch on the page picks one.
public theme ShopTheme {
    color primary: Color = #0F766E
    color onPrimary: Color = #FFFFFF
    color surface: Color = #FFFFFF
    color text: Color = #1C1917
    color muted: Color = #57534E

    space sm: Spacing = 8px
    space md: Spacing = 16px
    space lg: Spacing = 24px

    radius md: Radius = 8px

    variant dark matches .prefersDark {
        primary = #2DD4BF
        onPrimary = #042F2E
        surface = #1C1917
        text = #F5F5F4
        muted = #A8A29E
    }
}
