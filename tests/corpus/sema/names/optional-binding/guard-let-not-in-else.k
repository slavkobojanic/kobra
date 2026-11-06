// The `else` block of a `guard let` runs when the value is absent, so the binding isn't visible there.
func length(of text: String?) -> Int {
    guard let unwrapped = text else {
        return unwrapped.count
    }
    return unwrapped.count
}
