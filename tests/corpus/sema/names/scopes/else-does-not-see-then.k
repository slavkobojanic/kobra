// The `else` block doesn't see the `then` block's bindings.
func pick(flag: Bool) -> Int {
    if flag {
        let chosen = 1
        return chosen
    } else {
        return chosen
    }
}
