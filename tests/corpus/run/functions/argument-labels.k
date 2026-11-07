// External labels, `_` and internal names: the call uses labels, the body uses internal names.
func move(from start: Int, to end: Int) -> Int {
    return end - start
}

func scale(_ value: Int, by factor: Int) -> Int {
    return value * factor
}

func main() -> List<Int> {
    return [move(from: 2, to: 10), scale(3, by: 4)]
}
