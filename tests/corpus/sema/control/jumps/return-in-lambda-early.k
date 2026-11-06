// An early `return` in a lambda (not its last statement) is K2308 too; there
// is no simple fix, so the case pins none.
func positives(values: List<Int>) -> List<Int> {
    return values.map { value =>
        if value < 0 {
            return 0
        }
        value
    }
}
