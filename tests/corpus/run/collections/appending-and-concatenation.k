// `appending` and `+` return new lists and leave their operands unchanged.
func main() -> List<List<Int>> {
    let a = [1, 2]
    let b = [3]
    let none: List<Int> = []
    return [a.appending(9), a + b, b + a, a + none, a]
}
