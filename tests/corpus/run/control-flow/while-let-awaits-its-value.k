// `while let` binds the value its condition awaits, every iteration.
func next(_ n: Int) async -> Int? {
    if n < 3 {
        return n + 1
    }
    return nil
}

func main() async -> List<Int> {
    var seen: List<Int> = []
    var at = 0
    while let n = await next(at) {
        seen.append(n)
        at = n
    }
    return seen
}
