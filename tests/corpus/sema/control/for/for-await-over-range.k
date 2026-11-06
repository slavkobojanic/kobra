// A range is a collection too: K2311 with the same fix.
func total() async -> Int {
    var sum = 0
    for await value in 0..<10 {
        sum += value
    }
    return sum
}
