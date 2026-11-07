// `chunked` makes consecutive chunks; the last may be shorter, and an empty list has no chunks.
func main() -> List<List<List<Int>>> {
    let values = [1, 2, 3, 4, 5]
    let none: List<Int> = []
    return [values.chunked(2), values.chunked(5), values.chunked(10), none.chunked(3)]
}
