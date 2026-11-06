/// One point of a series.
public struct ChartPoint<X> implements Equatable where X implements ChartScalable {
    /// The x value.
    let x: X
    /// The y value.
    let y: Double
}
