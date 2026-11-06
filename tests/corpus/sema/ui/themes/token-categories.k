// A token declares a static member of its category's type: `color` → `Color`,
// `space` → `Spacing`, `radius` → `Radius`, `fontSize` → `FontSize`, `font` →
// `Font`.
theme Tokens {
    color primary: Color = #3B82F6
    //    ^^^^^^^ resolves: token Color.primary
    space md: Spacing = 16px
    //    ^^ resolves: token Spacing.md
    radius lg: Radius = 16px
    fontSize small: FontSize = 0.875rem
    font body: Font = Font(family: "Inter", size: 16px)
}
