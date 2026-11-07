// `prefix` takes at most `count` elements.
func main() -> List<List<Int>> {
    let values = [1, 2, 3]
    return [values.prefix(2), values.prefix(0), values.prefix(10)]
}
