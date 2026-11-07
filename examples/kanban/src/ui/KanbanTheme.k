/// Design tokens for Kanban, with three looks: the base (light), a dark
/// variant that also follows the system's dark mode until the user picks,
/// and a high-contrast variant.
///
/// `k.charts` and `k.drag` declare tokens of their own; the theme sets
/// them with the assignment form, so the progress chart, the drop
/// indicator and the drop target outline follow every variant.
public theme KanbanTheme {
    color primary: Color = #4F46E5
    color onPrimary: Color = #FFFFFF
    color background: Color = #EEF2F7
    color surface: Color = #FFFFFF
    color column: Color = #E2E8F0
    color text: Color = #0F172A
    color muted: Color = #475569
    color border: Color = #CBD5E1
    color danger: Color = #B91C1C
    color high: Color = #C2410C
    color low: Color = #0F766E
    color warningSurface: Color = #FEF3C7
    color warningText: Color = #78350F

    color chartDone: Color = #16A34A
    color chartTotal: Color = #4F46E5

    chartGrid = #CBD5E1
    chartAxis = #475569
    dropIndicator = #4F46E5
    dropTarget = #A5B4FC

    space xs: Spacing = 4px
    space sm: Spacing = 8px
    space md: Spacing = 16px
    space lg: Spacing = 24px

    radius sm: Radius = 4px
    radius md: Radius = 8px

    fontSize sm: FontSize = 0.875rem
    fontSize lg: FontSize = 1.25rem
    fontSize xl: FontSize = 1.75rem

    font body: Font = Font(family: "Inter", size: 16px)

    variant dark matches .prefersDark {
        primary = #818CF8
        onPrimary = #0F172A
        background = #020617
        surface = #0F172A
        column = #1E293B
        text = #F1F5F9
        muted = #94A3B8
        border = #334155
        danger = #F87171
        high = #FB923C
        low = #2DD4BF
        warningSurface = #422006
        warningText = #FDE68A
        chartDone = #4ADE80
        chartTotal = #818CF8
        chartGrid = #334155
        chartAxis = #94A3B8
        dropIndicator = #818CF8
        dropTarget = #4338CA
    }

    variant contrast {
        primary = #000000
        onPrimary = #FFFFFF
        background = #FFFFFF
        surface = #FFFFFF
        column = #FFFFFF
        text = #000000
        muted = #000000
        border = #000000
        danger = #9F1239
        high = #9A3412
        low = #115E59
        chartDone = #000000
        chartTotal = #1D4ED8
        chartGrid = #000000
        chartAxis = #000000
        dropIndicator = #000000
        dropTarget = #1D4ED8
    }
}
