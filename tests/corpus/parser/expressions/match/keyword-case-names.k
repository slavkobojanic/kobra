// implicit case names may be keywords (`.default`, `.in`)
func f() {
    match m {
        case .default { }
        case .in(let x) { }
    }
}
