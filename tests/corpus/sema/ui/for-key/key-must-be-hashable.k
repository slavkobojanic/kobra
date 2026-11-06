// The key identifies an item across updates, so its type must be `Hashable`
// (K2009 at the key expression). A struct without `Hashable` doesn't qualify.
struct Point {
    let x: Int
}

component Points {
    @Prop let points: List<Point>

    view {
        for point in points key point {
            text { "{point.x}" }
        }
        for point in points key point.x {
            text { "{point.x}" }
        }
    }
}
