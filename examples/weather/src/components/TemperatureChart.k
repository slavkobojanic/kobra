import k.charts.ChartPoint
import k.charts.LineChart
import k.charts.LineSeries
import k.time.TimeZone
import weather.HourlyReading
import weather.UnitSystem

/// A line chart of temperature and feels-like temperature over the next 24 hours.
///
/// Series colors are the `chartTemperature` and `chartFeelsLike` theme
/// tokens; grid and axis colors come from the `chartGrid` and `chartAxis`
/// tokens that `k.charts` declares. The chart therefore follows the theme's
/// dark variant without any code here.
public component TemperatureChart {
    /// The hours to plot, earliest first.
    @Prop let hours: List<HourlyReading>

    /// The unit system the readings are expressed in, used for the axis title.
    @Prop let units: UnitSystem

    /// The time zone hour labels are shown in.
    @Prop let timeZone: TimeZone

    @Derived let temperature: List<ChartPoint<Date>> = hours.map { hour =>
        ChartPoint(x: hour.time, y: hour.temperature)
    }

    @Derived let feelsLike: List<ChartPoint<Date>> = hours.map { hour =>
        ChartPoint(x: hour.time, y: hour.feelsLike)
    }

    view {
        figure {
            LineChart(
                series: [
                    LineSeries(name: "Temperature", points: temperature)
                        .color(.chartTemperature),
                    LineSeries(name: "Feels like", points: feelsLike)
                        .color(.chartFeelsLike)
                        .dashed(true)
                ],
                description: "Temperature and feels-like temperature for the next 24 hours"
            )
            .yAxisTitle(units.temperatureSymbol)
            .timeZone(timeZone)
            .height(240px)

            caption { "Next 24 hours" }
                .class(.caption)
        }
        .class(.chart)
    }

    style {
        .chart {
            margin: 0;
            padding: .lg;
            borderRadius: .lg;
            backgroundColor: .surfaceRaised;
        }

        .caption {
            marginTop: .sm;
            fontSize: .sm;
            color: .muted;
        }
    }
}
