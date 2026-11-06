// `break` in a `guard` outside a loop is K2309; the `guard` is not also K2305.
func check(ready: Bool) {
    guard ready else {
        break
    }
}
