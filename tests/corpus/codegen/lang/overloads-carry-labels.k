// Methods that share a base name are emitted with their label signature, like the stdlib's
// `contains` and `contains$where` (docs/compiler/codegen.md §4).
struct Bag {
    let items: List<Int>

    func find(_ item: Int) -> Bool {
        return items.contains(item)
    }

    func find(where predicate: (Int) -> Bool) -> Bool {
        return items.any(predicate)
    }
}

func both(bag: Bag) -> List<Bool> {
    return [bag.find(3), bag.find { n => n > 0 }]
}
