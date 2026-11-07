// Going below the smallest safe integer panics, like going above the largest.
func main() -> Int {
    let small = -9007199254740991
    return small - 1
}
