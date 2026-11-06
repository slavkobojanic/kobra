// A lambda parameter shadows an outer local of the same name inside the lambda.
func shifted(values: List<Int>) -> List<Int> {
    let value = 10
    let shifted = values.map { value => value + 1 }
    //                                  ^^^^^ resolves: local value 4:32
    return shifted.appending(value)
    //                       ^^^^^ resolves: local value 3:9
}
