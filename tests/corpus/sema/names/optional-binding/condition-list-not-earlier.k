// A condition can't see the bindings of later conditions.
func positive(value: Int?) -> Bool {
    if checked > 0, let checked = value {
        return true
    }
    return false
}
