import k.time.TimeZone

/// A line chart on SVG with an accessible data table
/// (`docs/design/37-charts.md`). The description is required, like
/// `image(description:)`.
@Native
public component LineChart<X> {
    /// The lines.
    @Prop let series: List<LineSeries<X>>
    /// The text description of what the chart shows.
    @Prop let description: String
    /// The height.
    @Prop let height: Length = 320px
    /// The y axis title.
    @Prop let yAxisTitle: String? = nil
    /// The x axis title.
    @Prop let xAxisTitle: String? = nil
    /// The y range; automatic when `nil`.
    @Prop let yRange: ClosedRange<Double>? = nil
    /// The time zone of `Date` axes.
    @Prop let timeZone: TimeZone = .local
    /// Shows the legend.
    @Prop let legend: Bool = true
}
