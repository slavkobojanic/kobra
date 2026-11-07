// A stdlib struct's memberwise initializer fills in the defaults its
// fields declare, as a project struct's does (`k.charts.LineSeries`).
import k.charts.ChartPoint
import k.charts.LineSeries

func main() -> List<Bool> {
    let plain = LineSeries(name: "Temperature", points: [ChartPoint(x: 1, y: 2.0)])
    let dashed = LineSeries(name: "Feels like", points: [ChartPoint(x: 1, y: 1.5)], dashed: true)
    let chained = LineSeries(name: "Chained", points: [ChartPoint(x: 1, y: 1.0)]).showsPoints(true)

    return [plain.color == nil, plain.dashed, plain.showsPoints, dashed.dashed, dashed.showsPoints, chained.dashed, chained.showsPoints]
}
