// An `if let` binding ends with the `then` block.
func length(of text: String?) -> Int {
    if let unwrapped = text {
        return unwrapped.count
    }
    return unwrapped.count
}
