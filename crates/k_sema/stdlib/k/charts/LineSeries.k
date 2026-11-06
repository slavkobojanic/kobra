/// One line of a `LineChart`, a `@Builder` struct (`docs/design/37-charts.md`).
@Builder
public struct LineSeries<X> where X implements ChartScalable {
    /// The name shown in the legend and the data table.
    let name: String
    /// The points.
    let points: List<ChartPoint<X>>
    /// The color; the next `chartSeries` token by default.
    let color: Color? = nil
    /// Draws a dashed line.
    let dashed: Bool = false
    /// Draws a marker at each point.
    let showsPoints: Bool = false
}
