# Charts

## Decision

`k.charts` provides chart components built on SVG and themed by tokens. Each chart **requires a text description**, like `image`, and renders an accessible data table alongside the graphic. The line chart is specified here; the other kinds (bar, area, pie, scatter) follow the same pattern.

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

## `LineChart`

- **`LineChart` is a stdlib component.** Its required props are `series: List<LineSeries<X>>` and `description: String`. It renders an SVG with `role="img"`, plus a visually hidden data table generated from the series.
- **Chained options:** `height: Length = 320px`, `yAxisTitle: String?`, `xAxisTitle: String?`, `yRange: ClosedRange<Double>?` (automatic by default), `timeZone: TimeZone = .local` (for `Date` axes), `legend: Bool = true`.
- **`LineSeries<X>` is a `@Builder` struct:** required `name: String` and `points: List<ChartPoint<X>>`; chained `color: Color?` (defaults to the next `chartSeries` token), `dashed: Bool = false` and `showsPoints: Bool = false`. Boolean options take their value: `.dashed(true)` ([01-principles.md](01-principles.md)).
- **`ChartPoint<X>(x: X, y: Double)`, where `X implements ChartScalable`.** The stdlib makes `Int`, `Double` and `Date` conform. `Date` axes get time ticks formatted in `timeZone`. `.local` is the browser's zone, which server rendering learns from the `k_tz` cookie ([35-time.md](35-time.md#server-rendering)).
- **The series are data, not slots.** Charts need to inspect series values to compute scales, and slots are opaque ([04-components.md](04-components.md)).

## Theming

`k.charts` declares the tokens it needs, with defaults, as libraries do ([07-themes-and-units.md](07-themes-and-units.md#library-tokens)):

| Token | Used for |
|---|---|
| `color chartAxis` | axis lines, tick labels, axis titles |
| `color chartGrid` | grid lines |
| `color chartSeries1` … `chartSeries6` | default series colors, in order |
| `fontSize chartLabel` | tick and legend labels |

- **The app overrides a library token with the assignment form** in its `theme`: `chartGrid = #E2E8F0`. Declaring `color chartGrid: Color = …` again is a compile error with a quick fix to the assignment.
- Variants override chart tokens like any other, so charts follow dark mode with no chart code.
- Series colors given with `.color(...)` are ordinary `Color` values, usually app tokens, so they switch with variants too.
- Charts render tokens as CSS custom properties. Switching variants restyles a chart without re-rendering it.

## Why

- Making the accessible description required follows the `image(description:)` precedent.
- Tokens declared by the library and overridden by assignment keep "one theme per app" while letting libraries be themed, without a declaration that silently means "override".
