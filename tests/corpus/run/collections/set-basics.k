// Sets drop duplicates; `insert` and `remove` are idempotent; `contains` tests membership.
func main() -> List<String> {
    var tags: Set<String> = Set(["new", "sale", "new"])
    let before = tags.count
    tags.insert("sale")
    tags.insert("hot")
    tags.remove("new")
    tags.remove("missing")
    let empty: Set<Int> = Set([])
    return ["{before}", "{tags.count}", "{tags.contains("hot")}", "{tags.contains("new")}", "{empty.isEmpty}"]
}
