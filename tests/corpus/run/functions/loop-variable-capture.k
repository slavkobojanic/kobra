// Each loop iteration has its own binding, so lambdas made in a loop remember their own value.
func main() -> List<Int> {
    var makers: List<() -> Int> = []
    for i in 0..<3 {
        makers.append({ i * 10 })
    }
    return makers.map { make => make() }
}
