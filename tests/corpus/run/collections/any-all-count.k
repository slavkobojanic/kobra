// `any` is false and `all` is true on an empty list; `count(where:)` counts matches.
func main() -> List<String> {
    let values = [1, 2, 3]
    let none: List<Int> = []
    return [
        "{values.any { v => v > 2 }}",
        "{values.all { v => v > 0 }}",
        "{values.all { v => v > 1 }}",
        "{none.any { v => v > 0 }}",
        "{none.all { v => v > 0 }}",
        "{values.count(where: { v => v != 2 })}",
    ]
}
