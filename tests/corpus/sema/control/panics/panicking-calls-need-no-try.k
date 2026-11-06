// Panics (index out of range, a failed removal) are not part of any type: the
// calls that can panic are ordinary, non-throwing calls.
func takeFirst(items: List<Int>) -> Int {
    var copy = items
    let removed = copy.remove(at: 0)
//      ^^^^^^^ type: Int
    return removed / items.count
}
