// A `var` property requirement is satisfied by a stored `let` field or a computed property of the same type.
interface Priced {
    var price: Int
}

struct Mug implements Priced {
    let price: Int
}

struct Bundle implements Priced {
    let parts: List<Int>
    var price: Int { parts.count * 10 }
}

func cost(item: Priced) -> Int {
    return item.price
              //^^^^^ resolves: member corpus.Priced.price
}
