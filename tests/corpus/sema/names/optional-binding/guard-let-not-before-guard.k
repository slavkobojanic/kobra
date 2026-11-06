// A `guard let` binding is not visible before the `guard`.
func length(of text: String?) -> Int {
    let early = unwrapped
    guard let unwrapped = text else { return 0 }
    return unwrapped.count
}
