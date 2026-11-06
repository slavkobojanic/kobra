/// The app's tokens and variants.
public theme AppTheme {
    color surface: Color = #FFFFFF
    color text: Color = #111827
    space md: Spacing = 16px

    variant dark matches .prefersDark {
        surface = #111827
        text = #F9FAFB
    }

    variant highContrast {
        text = #000000
    }
}
