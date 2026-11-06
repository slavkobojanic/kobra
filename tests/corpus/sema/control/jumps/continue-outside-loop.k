// `continue` outside a loop is K2309 at the keyword.
func check(ready: Bool) {
    if ready {
        continue
    }
}
