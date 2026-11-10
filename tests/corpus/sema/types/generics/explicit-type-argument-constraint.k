// An explicit type argument is checked against its parameter's `where`
// constraints (K2009 at the argument), and the arguments of the call are
// then checked against it (K2001).
struct Rock {
    let weight: Int
}

func largest<T>(_ items: List<T>) -> T? where T implements Comparable {
    return items.first
}

func first<T>(_ items: List<T>) -> T? {
    return items.first
}

let heaviest = largest<Rock>([])
let name = first<String>([1, 2])
