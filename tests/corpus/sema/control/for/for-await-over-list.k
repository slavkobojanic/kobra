// `for await` over a collection is K2311 at the `await` token, with a
// machine-applicable fix removing `await `.
func total(values: List<Int>) async -> Int {
    var sum = 0
    for await value in values {
        sum += value
    }
    return sum
}
