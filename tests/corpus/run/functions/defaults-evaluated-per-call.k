// A default argument is evaluated on each call that omits it, so a default list is never shared.
func appended(value: Int, to list: List<Int> = []) -> List<Int> {
    var copy = list
    copy.append(value)
    return copy
}

func main() -> List<List<Int>> {
    return [appended(value: 1), appended(value: 2), appended(value: 3, to: [0])]
}
