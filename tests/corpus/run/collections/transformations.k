// `filter`, `map`, `compactMap` and `flatMap` keep order and return new lists.
func main() -> List<List<Int>> {
    let numbers = [1, 2, 3, 4]
    let none: List<Int> = []
    return [
        numbers.filter { n => n % 2 == 0 },
        numbers.map { n => n * 10 },
        numbers.flatMap { n => [n, n] },
        none.map { n => n + 1 },
        numbers.filter { n => n > 10 },
    ]
}
