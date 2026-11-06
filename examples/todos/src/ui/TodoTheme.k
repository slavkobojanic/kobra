/// Design tokens for the todos app, with a dark variant.
///
/// Every color, space, radius and font size in the app's `style` blocks comes
/// from here. The `dark` variant overrides only colors, so spacing and type
/// stay identical in both schemes. Switching variants at runtime, through the
/// stdlib's Theme service, swaps one class on the document root and
/// re-renders nothing.
public theme TodoTheme {
    color primary: Color = #2563EB
    color onPrimary: Color = #FFFFFF
    color background: Color = #F1F5F9
    color surface: Color = #FFFFFF
    color text: Color = #0F172A
    color muted: Color = #64748B
    color border: Color = #E2E8F0
    color danger: Color = #B91C1C
    color warningSurface: Color = #FEF3C7
    color warningText: Color = #78350F

    space xs: Spacing = 4px
    space sm: Spacing = 8px
    space md: Spacing = 16px
    space lg: Spacing = 24px

    radius sm: Radius = 4px
    radius md: Radius = 8px

    fontSize sm: FontSize = 0.875rem
    fontSize xl: FontSize = 1.75rem

    font body: Font = Font(family: "Inter", size: 16px)

    variant dark {
        primary = #60A5FA
        onPrimary = #0F172A
        background = #020617
        surface = #0F172A
        text = #F1F5F9
        muted = #94A3B8
        border = #334155
        danger = #F87171
        warningSurface = #422006
        warningText = #FDE68A
    }
}
