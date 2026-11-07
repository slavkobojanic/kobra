// The safe-integer bounds themselves are valid Ints, also when reached by arithmetic.
func main() -> List<Int> {
    let below = 9007199254740990
    let above = -9007199254740990
    return [9007199254740991, -9007199254740991, below + 1, above - 1]
}
