// `break` in a `for await` loop closes the stream; it's an ordinary loop jump.
func firstPositive(values: Stream<Int>) async -> Int {
    for await value in values {
        if value > 0 {
            break
        }
    }
    return 0
}
