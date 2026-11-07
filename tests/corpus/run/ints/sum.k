// `sum()` adds a `List<Int>`; the empty sum is 0.
func main() -> List<Int> {
    let none: List<Int> = []
    return [[1, 2, 3].sum(), none.sum(), [-4, 4].sum()]
}
