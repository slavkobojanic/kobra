// `for` counts over half-open and closed ranges; a range whose end is not above its start runs zero times.
func main() -> List<Int> {
    var out: List<Int> = []
    for i in 1...3 {
        out.append(i)
    }
    for i in 3..<3 {
        out.append(i * 100)
    }
    for i in 5..<2 {
        out.append(i * 1000)
    }
    for _ in 0..<2 {
        out.append(-1)
    }
    return out
}
