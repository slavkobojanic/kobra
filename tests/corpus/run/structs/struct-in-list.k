// Structs in a list are values: a copy read out of the list is independent, and writing through the list changes only the list.
struct Point implements Codable {
    var x: Int
    var y: Int
}

func main() -> List<List<Point>> {
    var points = [Point(x: 1, y: 1), Point(x: 2, y: 2)]
    let snapshot = points
    var first = points[0]
    first.x = 100
    points[1].y = 20
    return [snapshot, points, [first]]
}
