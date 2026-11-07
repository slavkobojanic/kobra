/// The design tokens for the K Shop storefront.
///
/// Every color, spacing and radius in the app's `style` blocks comes from
/// here, so a rebrand changes one file. Tokens become static members of their
/// type, which is why styles can write `color: .muted;` and
/// `padding: .md;`.
public theme ShopTheme {
    color primary: Color = #0F766E
    color surfaceSunken: Color = #F5F5F4
    color border: Color = #E7E5E4
    color text: Color = #1C1917
    color muted: Color = #78716C
    color danger: Color = #B91C1C

    space sm: Spacing = 8px
    space md: Spacing = 16px
    space lg: Spacing = 32px

    radius md: Radius = 8px

    font body: Font = Font(family: "Inter", size: 16px)
}
