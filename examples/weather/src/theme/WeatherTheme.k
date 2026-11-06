/// Design tokens for the weather dashboard, with a dark variant that follows
/// the operating system's color scheme.
///
/// Every color, space and radius in the app's `style` blocks comes from here.
/// The `chartGrid` and `chartAxis` tokens are declared by `k.charts`, so this
/// theme overrides them with the assignment form instead of declaring them
/// again; that restyles every chart without touching the chart code.
///
/// The app has no theme switch. `matches .prefersDark` applies the dark
/// variant through a CSS media query, so even the server-rendered first paint
/// is in the right scheme.
public theme WeatherTheme {
    color primary: Color = #2563EB
    color surface: Color = #FFFFFF
    color surfaceRaised: Color = #F1F5F9
    color text: Color = #0F172A
    color muted: Color = #64748B
    color border: Color = #E2E8F0
    color positive: Color = #15803D
    color caution: Color = #B45309
    color danger: Color = #B91C1C

    color chartTemperature: Color = #EA580C
    color chartFeelsLike: Color = #2563EB

    chartGrid = #E2E8F0
    chartAxis = #64748B

    space xs: Spacing = 4px
    space sm: Spacing = 8px
    space md: Spacing = 16px
    space lg: Spacing = 24px
    space xl: Spacing = 40px

    radius md: Radius = 8px
    radius lg: Radius = 16px

    fontSize sm: FontSize = 0.875rem
    fontSize lg: FontSize = 1.25rem
    fontSize display: FontSize = 3.5rem

    font body: Font = Font(family: "Inter", size: 16px)

    variant dark matches .prefersDark {
        surface = #0F172A
        surfaceRaised = #1E293B
        text = #F1F5F9
        muted = #94A3B8
        border = #334155
        chartGrid = #334155
        chartAxis = #94A3B8
    }
}
