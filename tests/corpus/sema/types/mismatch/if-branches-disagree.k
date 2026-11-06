// An `if` used as a value takes its type from the first branch; a disagreeing `else` is K2001 at its value.
func label(done: Bool) -> String {
    let text = if done { "yes" } else { 0 }
    return text
}
