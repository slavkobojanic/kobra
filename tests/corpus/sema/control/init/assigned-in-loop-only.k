// A loop may run zero times: an assignment only inside it doesn't initialize
// the binding for the read after the loop.
func last(values: List<Int>) -> Int {
    var result: Int
    for value in values {
        result = value
    }
    return result
}
