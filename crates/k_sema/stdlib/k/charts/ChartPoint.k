/// One point of a series ([`LineSeries.points`]).
public struct ChartPoint<X> implements Equatable where X implements ChartScalable {
    /// The position on the x axis.
    let x: X
    /// The value on the y axis.
    let y: Double
}
