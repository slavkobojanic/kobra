# Proposal: `k.charts` Line Charts

> **Status: proposal.** Not yet accepted into the design. First used by `examples/weather`.

## Problem

[20-standard-library.md](../20-standard-library.md) says `k.charts` provides "line, bar, area, pie and scatter charts, built on SVG and themed by tokens", but defines neither the API nor how tokens apply.

## Proposal

### API

```k
import k.charts.ChartPoint
import k.charts.LineChart
import k.charts.LineSeries

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
.yAxisTitle("°C")
.timeZone(timeZone)
.height(240px)
```

- **`LineChart` is a stdlib component.** Its required props are `series: List<LineSeries<X>>` and `description: String`. Like `image`, a chart **requires a text description** for screen readers. It renders as an SVG with `role="img"`, plus a visually hidden data table generated from the series.
- **Chained options:** `height: Length = 320px`, `yAxisTitle: String?`, `xAxisTitle: String?`, `yRange: ClosedRange<Double>?` (auto by default), `timeZone: TimeZone = .local` (for `Date` axes), `legend: Bool = true`.
- **`LineSeries<X>` is a `@Builder` struct:** required `name: String` and `points: List<ChartPoint<X>>`, chained `color: Color?` (defaults to the next `chartSeries` token), `dashed: Bool = false` and `showsPoints: Bool = false`.
- **`ChartPoint<X>(x: X, y: Double)`, where `X implements ChartScalable`.** The stdlib makes `Int`, `Double` and `Date` conform. `Date` axes get time ticks formatted in `timeZone`.
- The series are data, not slots. Charts need to inspect series values to compute scales, and slots are opaque ([04-components.md](../04-components.md)).

### Theming

`k.charts` declares the tokens it needs, with stdlib defaults, as [07-themes-and-units.md](../07-themes-and-units.md) requires of libraries:

| Token | Used for |
|---|---|
| `color chartAxis` | axis lines, tick labels, axis titles |
| `color chartGrid` | grid lines |
| `color chartSeries1` … `chartSeries6` | default series colors, in order |
| `fontSize chartLabel` | tick and legend labels |

- **An app overrides a library token by declaring it in its own `theme`** with the same category and type (`color chartGrid: Color = #E2E8F0`). Variants override it like any other token, so charts follow dark mode with no chart code.
- Series colors given with `.color(...)` are ordinary `Color` values, usually app tokens, so they switch with variants too.
- Charts render tokens as CSS custom properties. Switching variants restyles a chart without re-rendering it.

## Why

- Making the accessible description required follows the `image(description:)` precedent.
- Tokens declared by the library and overridden by the app keep "one theme per app" while letting libraries be themed.
