// `sorted()` orders numbers numerically and strings by code units; the original is unchanged.
func main() -> List<List<String>> {
    let numbers = [10, 9, 100, -1]
    let words = ["b", "B", "a", "ab"]
    let none: List<Int> = []
    return [numbers.sorted().map { n => "{n}" }, words.sorted(), none.sorted().map { n => "{n}" }, numbers.map { n => "{n}" }]
}
