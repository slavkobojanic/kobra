// A negative index panics; there is no indexing from the end.
func main() -> Int {
    let values = [1, 2, 3]
    let index = -1
    return values[index]
}
