// A `guard let` binding lasts until the end of the block that holds the `guard`.
func length(of text: String?, enabled: Bool) -> Int {
    if enabled {
        guard let unwrapped = text else {
            return 0
        }
        return unwrapped.count
    }
    return unwrapped.count
}
