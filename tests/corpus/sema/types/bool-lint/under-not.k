// `not done == true` is `not (done == true)`; the fix leaves `not done`.
func check(done: Bool) -> Bool {
    return not done == true
}
