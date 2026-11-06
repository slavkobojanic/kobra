// Outside an async function, K2310 is still reported, but `for await` would be
// wrong there too, so the fix isn't machine-applicable (pinned only by code).
func consume(values: Stream<Int>) -> Int {
    var sum = 0
    for value in values {
        sum += value
    }
    return sum
}
