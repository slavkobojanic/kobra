// A `return` inside a loop doesn't cover the path where the loop ends.
func firstEven(values: List<Int>) -> Int {
    for value in values {
        if value % 2 == 0 {
            return value
        }
    }
}
