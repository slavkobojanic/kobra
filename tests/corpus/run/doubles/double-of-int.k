// `Double(_)` of an `Int` is the same number as a `Double`, the conversion a chart of counts needs.
func main() -> List<Double> {
    let cards = 3
    return [Double(cards), Double(cards) / 2.0]
}
