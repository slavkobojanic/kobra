// Map and Set compare struct keys by value, not identity (docs/compiler/codegen.md §5).
struct Point implements Hashable {
    let x: Int
    let y: Int
}

func main() -> List<Int> {
    var visits: Map<Point, Int> = [:]
    visits[Point(x: 1, y: 2)] = 1
    visits[Point(x: 1, y: 2)] = 5
    let seen = Set([Point(x: 0, y: 0), Point(x: 0, y: 0), Point(x: 1, y: 0)])
    return [visits.count, visits[Point(x: 1, y: 2)] ?? 0, seen.count]
}
