// The tokens a library declares (`k.charts`' `ChartTokens`) are custom properties on `:root` with
// their defaults once the project uses the library, and the theme's assignment form overrides them
// in `:root` itself, not only in variants; a library the project doesn't use adds nothing
// (07-themes-and-units.md, "Library tokens"; codegen.md §12).
import k.charts.ChartPoint
import k.charts.LineChart
import k.charts.LineSeries

theme Tokens {
    color text: Color = #0F172A
    chartGrid = #CBD5E1

    variant dark {
        text = #F1F5F9
        chartGrid = #334155
        chartLabel = 0.875rem
    }
}

component Sparkline {
    @Prop let values: List<Double>

    @Derived let points: List<ChartPoint<Int>> = (0..<values.count).map { i => ChartPoint(x: i, y: values[i]) }

    view {
        LineChart(series: [LineSeries(name: "Values", points: points)], description: "Recent values")
    }
}
