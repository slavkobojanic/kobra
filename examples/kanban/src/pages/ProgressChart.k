import board.Board
import k.charts.ChartPoint
import k.charts.LineChart
import k.charts.LineSeries

/// The board's progress over time (`k.charts`): its done cards and all of
/// its cards after each change, from the board's history.
///
/// The series colors are the theme's `chartDone` and `chartTotal` tokens,
/// so the chart follows the dark and high-contrast variants. The chart's
/// description is required, and it renders an accessible data table.
component ProgressChart {
    /// The board.
    @Prop let board: Board

    @Derived let done: List<ChartPoint<Date>> = board.history.map { point =>
        ChartPoint(x: point.at, y: Double(point.done))
    }

    @Derived let total: List<ChartPoint<Date>> = board.history.map { point =>
        ChartPoint(x: point.at, y: Double(point.total))
    }

    @Derived let summary: String = "{board.doneCount} of {board.cardCount} cards done"

    view {
        figure {
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

            caption { "Progress: {summary}" }
                .class(.caption)
        }
        .class(.chart)
    }

    style {
        .chart {
            margin: 0;
            padding: .md;
            borderRadius: .md;
            backgroundColor: .surface;
            border: 1px solid .border;
        }

        .caption { marginTop: .sm; color: .muted; fontSize: .sm; }
    }
}
