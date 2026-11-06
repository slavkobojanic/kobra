// In a condition list, each condition sees the bindings of the conditions before it.
struct Node {
    let next: Node?
    let value: Int
}

func second(node: Node?) -> Int {
    if let first = node, let after = first.next, after.value > 0 {
    //                               ^^^^^ resolves: local first 8:12
    //                                           ^^^^^ resolves: local after 8:30
        return after.value
    }
    return 0
}
