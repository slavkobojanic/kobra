// An unresolved initializer is one K1201: the binding is `{error}`, and its uses report nothing more.
func length() -> Int {
    if let unwrapped = missing {
        return unwrapped.count
    }
    return 0
}
