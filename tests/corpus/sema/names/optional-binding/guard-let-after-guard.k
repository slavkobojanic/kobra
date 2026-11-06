// `guard let` binds for the rest of the enclosing block, after the `guard`.
func length(of text: String?) -> Int {
    guard let unwrapped = text else {
        return 0
    }
    return unwrapped.count
    //     ^^^^^^^^^ resolves: local unwrapped 3:15
}
