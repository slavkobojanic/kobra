// `case { }` is K0124 at the `{`
func f() {
    match v {
        case { }
        default { }
    }
}
