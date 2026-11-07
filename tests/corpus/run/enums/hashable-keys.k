// Hashable enums with payloads work as Set elements and Map keys by value.
enum Cell implements Hashable {
    case empty
    case filled(row: Int, column: Int)
}

func main() -> List<Int> {
    let cells: Set<Cell> = Set([.empty, .filled(row: 1, column: 2), .filled(row: 1, column: 2), .empty])
    var hits: Map<Cell, Int> = [:]
    hits[.filled(row: 0, column: 0)] = 1
    hits[.filled(row: 0, column: 0)] = 2
    hits[.empty] = 3
    return [cells.count, hits.count, hits[.filled(row: 0, column: 0)] ?? 0]
}
