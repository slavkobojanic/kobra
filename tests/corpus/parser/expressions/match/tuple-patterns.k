// tuple patterns of literals, wildcards, bindings and cases
func f() {
    match (a, b) {
        case (0, _) { }
        case (let x, .some(let y)) { }
        case (1, 2, 3,) { }
    }
}
