// A lambda parameter is out of scope after the lambda's `}`.
func doubled(values: List<Int>) -> Int {
    let all = values.map { value => value * 2 }
    return all.count + value
}
