// `remove(at:)` past the end panics.
func main() -> Int {
    var values = [1]
    return values.remove(at: 1)
}
