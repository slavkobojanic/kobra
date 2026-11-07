// Reading past the end of a list panics.
func main() -> Int {
    let values = [1, 2, 3]
    return values[3]
}
