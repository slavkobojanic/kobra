// A `guard` inside a loop may `continue` or `break`.
func main() -> List<Int> {
    var out: List<Int> = []
    for n in [1, -2, 3, 0, 5] {
        guard n != 0 else {
            break
        }
        guard n > 0 else {
            continue
        }
        out.append(n)
    }
    return out
}
