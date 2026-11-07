// `break` and `continue` apply to the innermost loop.
func main() -> List<String> {
    var out: List<String> = []
    for i in 0..<3 {
        for j in 0..<3 {
            if j == 1 {
                continue
            }
            if j > i {
                break
            }
            out.append("{i}{j}")
        }
    }
    return out
}
