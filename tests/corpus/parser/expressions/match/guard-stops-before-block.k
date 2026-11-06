// a match guard is `expr_nt`: `where xs.isEmpty { }` does not take a trailing lambda
func f() {
    match x {
        case .a where xs.isEmpty { }
    }
}
