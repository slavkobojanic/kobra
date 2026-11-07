// Sets have value semantics like lists.
func main() -> List<Int> {
    let original: Set<Int> = Set([1, 2])
    var copy = original
    copy.insert(3)
    copy.remove(1)
    return [original.count, copy.count, if original.contains(1) { 1 } else { 0 }]
}
