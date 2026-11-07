# Charts

`k.charts` draws charts as SVG, themed by tokens. Like `image`, **every chart requires a text
description**, and it renders an accessible data table next to the graphic, so a chart is never a
picture that some readers can't read.

Today `k.charts` has one chart, the line chart. Here is the one from the weather example:

```k
// examples/weather/src/components/TemperatureChart.k
public component TemperatureChart {
    // ...
    @Prop let hours: List<HourlyReading>
    // ...
    @Prop let units: UnitSystem
    // ...
    @Prop let timeZone: TimeZone
    // ...
    @Derived let temperature: List<ChartPoint<Date>> = hours.map { hour =>
        ChartPoint(x: hour.time, y: hour.temperature)
    }
    // ...
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
            // ...
```

## The pieces

| Declaration | What it is |
|---|---|
| `LineChart<X>` | the component. Required: `series: List<LineSeries<X>>` and `description: String`. Chained: `height` (default `320px`), `yAxisTitle`, `xAxisTitle`, `yRange: ClosedRange<Double>?` (automatic by default), `timeZone` (default `.local`, for `Date` axes) and `legend` (default `true`) |
| `LineSeries<X>` | one line, a `@Builder` struct: required `name` and `points`; chained `color: Color?`, `dashed: Bool` and `showsPoints: Bool` |
| `ChartPoint<X>(x:y:)` | one point: `x` is an `Int`, `Double` or `Date`, `y` a `Double` |
| `ChartScalable` | the interface an `x` type implements; the standard library makes `Int`, `Double` and `Date` conform |

- **The series are data, not slots.** A chart has to read the values to compute its scales, and slots
  are opaque to the component that receives them.
- **`Date` axes get time ticks**, formatted in the chart's `timeZone`. `.local` is the browser's zone;
  during server rendering it is the zone the browser reported in the `k_tz` cookie.
- **Boolean options take their value**: `.dashed(true)`, `.showsPoints(true)`.

Build the points with `@Derived` values, as above, so the chart redraws exactly when its data changes.
The Kanban board's progress chart plots two series over time:

```k
// examples/kanban/src/pages/ProgressChart.k
@Derived let done: List<ChartPoint<Date>> = board.history.map { point =>
    ChartPoint(x: point.at, y: Double(point.done))
}

@Derived let total: List<ChartPoint<Date>> = board.history.map { point =>
    ChartPoint(x: point.at, y: Double(point.total))
}
// ...
LineChart(
    series: [
        LineSeries(name: "Done", points: done)
            .color(.chartDone)
            .showsPoints(true),
        LineSeries(name: "All cards", points: total)
            .color(.chartTotal)
            .dashed(true)
    ],
    description: "Done cards and all cards of {board.title} over time"
)
.yAxisTitle("Cards")
.height(200px)
```

## Theming

`k.charts` declares the tokens it needs, with defaults, as a library does:

```k
// crates/k_sema/stdlib/k/charts/ChartTokens.k
public theme ChartTokens {
    // ...
    color chartAxis: Color = #64748B
    // ...
    color chartGrid: Color = #E2E8F0
```

| Token | Used for |
|---|---|
| `color chartAxis` | axis lines, tick labels and axis titles |
| `color chartGrid` | grid lines |
| `color chartSeries1` … `chartSeries6` | the default series colors |
| `fontSize chartLabel` | tick and legend labels |

- **A series without `.color(…)` takes `chartSeries1` … `chartSeries6` by its position**, wrapping
  after six.
- **The app overrides a library token with the assignment form** in its theme, `chartGrid = #E2E8F0`,
  and in variants too, so charts follow dark mode with no chart code. Declaring `color chartGrid: …`
  again is `K3302`, with a quick fix to the assignment ([Styles and themes](styles.md#library-tokens)).
- Series colors given with `.color(…)` are ordinary `Color` values, usually app tokens such as
  `.chartTemperature`, so they switch with variants as well.

> **Planned.** The design adds bar, area, pie and scatter charts that follow the same pattern. Only
> `LineChart` is built.
