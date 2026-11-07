// `contains(_:)` compares by value; `contains { … }` (`contains(where:)`) takes a predicate.
struct Point implements Equatable {
    let x: Int
    let y: Int
}

func main() -> List<Bool> {
    let points = [Point(x: 1, y: 2)]
    return [points.contains(Point(x: 1, y: 2)), points.contains(Point(x: 2, y: 1)), [1, 2].contains { n => n > 1 }, ["a"].contains("b")]
}
