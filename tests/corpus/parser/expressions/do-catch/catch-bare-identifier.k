// `catch error` is K0115: bindings need `let`
func f() {
    do { try a() } catch error { }
}
