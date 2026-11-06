// A type argument that doesn't implement the constraint is K2009 at the argument it was inferred from.
interface Priced {
    var price: Int
}

struct Rock {
    let weight: Int
}

func cheapest<T>(items: List<T>) -> T? where T implements Priced {
    return items.first
}

let rocks = [Rock(weight: 1)]
let rock = cheapest(items: rocks)
