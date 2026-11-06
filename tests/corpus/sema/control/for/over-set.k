// A `Set<T>` is iterated like a list.
func total(tags: Set<String>) -> Int {
    var sum = 0
    for tag in tags {
//      ^^^ type: String
        sum += tag.count
    }
    return sum
}
