// `LineChart` is the runtime's `k.charts` (runtime.md §12.2): it ships in a chunk of its own, imported
// from the runtime chunk, and only to apps that draw charts; the server renders it with
// `$s.natives` (codegen.md §16).
import k.charts.ChartPoint
import k.charts.LineChart
import k.charts.LineSeries

component Sparkline {
    @Prop let values: List<Double>

    @Derived let points: List<ChartPoint<Int>> = (0..<values.count).map { i => ChartPoint(x: i, y: values[i]) }

    view {
        LineChart(series: [LineSeries(name: "Values", points: points)], description: "Recent values")
            .height(120px)
    }
}
