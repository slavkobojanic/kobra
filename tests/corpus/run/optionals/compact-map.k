// `compactMap` drops nil results and keeps order.
func main() -> List<Int> {
    let words = ["1", "x", "3", "", "-2"]
    return words.compactMap { word => Int.parse(word) }
}
