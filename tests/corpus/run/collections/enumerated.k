// `enumerated` pairs each element with its index, destructured in `for`.
func main() -> List<String> {
    var out: List<String> = []
    for (index, word) in ["a", "b", "c"].enumerated() {
        out.append("{index}:{word}")
    }
    return out
}
