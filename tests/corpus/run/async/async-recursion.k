// Async functions can recurse.
func countdown(n: Int) async -> List<Int> {
    if n == 0 {
        return [0]
    }
    let rest = await countdown(n: n - 1)
    return [n] + rest
}

func main() async -> List<Int> {
    return await countdown(n: 3)
}
