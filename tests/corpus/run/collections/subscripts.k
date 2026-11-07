// Reading and writing elements by index.
func main() -> List<Int> {
    var values = [10, 20, 30]
    values[0] = 11
    values[2] += 3
    return [values[0], values[1], values[2], values[values.count - 1]]
}
