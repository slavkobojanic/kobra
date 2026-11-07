// Half-open and closed ranges map and test membership; an empty range has no values.
func main() -> List<List<Int>> {
    let empty = (5..<5).map { n => n }
    let flags = [(0..<3).contains(3), (0...3).contains(3), (1..<4).contains(0)].map { b => if b { 1 } else { 0 } }
    return [(0..<4).map { n => n * n }, (1...3).map { n => n }, empty, flags]
}
