/// One line of a [`LineChart`], a `@Builder` struct
/// (`docs/design/37-charts.md`).
@Builder
public struct LineSeries<X> where X implements ChartScalable {
    /// The name shown in the legend and the data table.
    let name: String
    /// The points of the line, in x order.
    let points: List<ChartPoint<X>>
    /// The line's color; when `nil`, the series token of [`ChartTokens`]
    /// for its position in the chart (`chartSeries1` to `chartSeries6`,
    /// then again from the first).
    let color: Color? = nil
    /// `true` to draw a dashed line.
    let dashed: Bool = false
    /// `true` to draw a marker at each point.
    let showsPoints: Bool = false
}
