// The `else` block runs when the value is absent, so the binding isn't visible there; `.count` reports nothing more.
func length(of text: String?) -> Int {
    if let unwrapped = text {
        return unwrapped.count
    } else {
        return unwrapped.count
    }
}
