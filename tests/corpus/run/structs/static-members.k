// Static lets and static functions belong to the type, not to a value.
struct Point implements Codable {
    let x: Int
    let y: Int

    static let origin = Point(x: 0, y: 0)

    static func diagonal(_ n: Int) -> Point {
        return Point(x: n, y: n)
    }
}

func main() -> List<Point> {
    let p: Point = .diagonal(3)
    return [Point.origin, p, .origin]
}
