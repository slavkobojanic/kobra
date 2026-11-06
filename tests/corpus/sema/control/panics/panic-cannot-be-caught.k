// A panicking call throws nothing, so a `do` around it has nothing to catch:
// its catch-all can never run (K2302 at the `catch` keyword).
func takeFirst(items: List<Int>) -> Int {
    var copy = items
    do {
        return copy.remove(at: 0)
    } catch _ {
        return 0
    }
}
