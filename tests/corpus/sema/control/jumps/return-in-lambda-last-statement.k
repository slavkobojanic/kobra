// `return` as a lambda's last statement is K2308 with a machine-applicable fix
// removing `return `: the lambda's value is its last expression.
func doubled(values: List<Int>) -> List<Int> {
    return values.map { value =>
        let twice = value * 2
        return twice
    }
}
