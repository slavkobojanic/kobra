// Maps and Sets keyed by structs use the structural `$k.HashMap`/`$k.HashSet`; primitive keys use
// JS `Map`/`Set` (docs/compiler/codegen.md §5).
struct Point implements Hashable {
    let x: Int
    let y: Int
}

func counts() -> List<Int> {
    let byPoint: Map<Point, Int> = [Point(x: 0, y: 0): 1]
    let byName: Map<String, Int> = ["a": 1]
    let points: Set<Point> = Set([Point(x: 1, y: 1)])
    return [byPoint.count, byName.count, points.count]
}
