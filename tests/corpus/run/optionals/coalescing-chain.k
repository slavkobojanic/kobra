// `??` is right-associative: the first non-nil value wins.
func main() -> List<Int> {
    let a: Int? = nil
    let b: Int? = nil
    let c: Int? = 3
    return [a ?? b ?? c ?? 0, a ?? b ?? 0, c ?? 9]
}
