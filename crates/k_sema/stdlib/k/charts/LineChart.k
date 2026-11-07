import k.time.TimeZone

/// A line chart on SVG with an accessible data table
/// (`docs/design/37-charts.md`). The description is required, like
/// `image(description:)`.
@Native
public component LineChart<X> {
    /// The lines, drawn in order; each is a [`LineSeries`].
    @Prop let series: List<LineSeries<X>>
    /// The text description of what the chart shows, read by assistive
    /// technology.
    @Prop let description: String
    /// The height of the chart; it takes the full available width.
    @Prop let height: Length = 320px
    /// The title of the y axis; none when `nil`.
    @Prop let yAxisTitle: String? = nil
    /// The title of the x axis; none when `nil`.
    @Prop let xAxisTitle: String? = nil
    /// The range of the y axis; computed from the data when `nil`.
    @Prop let yRange: ClosedRange<Double>? = nil
    /// The time zone in which [`k.core.Date`] values on the x axis are
    /// labelled.
    @Prop let timeZone: TimeZone = .local
    /// `true` to show the legend of the series names.
    @Prop let legend: Bool = true
}
