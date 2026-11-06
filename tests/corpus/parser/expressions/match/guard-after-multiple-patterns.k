// a `where` guard applies to all patterns of the arm
func f() {
    match x {
        case .a, .b where ok { }
    }
}
