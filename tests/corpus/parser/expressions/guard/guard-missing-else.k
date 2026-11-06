// a guard without `else` is K0100 at the `{`
func f() {
    guard ready { return }
}
