public theme ThemedTheme {
    color primary: Color = #2563EB
    color background: Color = #F1F5F9
    color surface: Color = #FFFFFF
    color text: Color = #0F172A

    font body: Font = Font(family: "Inter", size: 16px)

    variant dark matches .prefersDark {
        background = #020617
        surface = #0F172A
        text = #F1F5F9
    }

    variant highContrast {
        background = #FFFFFF
        text = #000000
    }
}
