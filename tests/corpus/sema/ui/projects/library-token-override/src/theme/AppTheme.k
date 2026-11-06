// The app overrides a token `k.charts` declares with the assignment form, at the
// top level and in a variant; the override resolves to the library's token.
public theme AppTheme {
    color surface: Color = #FFFFFF
    chartGrid = #E2E8F0
    //@ 5:5-5:14 resolves: token Color.chartGrid
    chartLabel = 0.8rem

    variant dark {
        surface = #111827
        chartGrid = #334155
    }
}
