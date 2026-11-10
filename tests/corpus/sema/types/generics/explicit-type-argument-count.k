// Explicit type arguments give one type per generic parameter (K2020), in
// expressions and in types alike; a name that isn't generic takes none,
// with a fix that removes them.
struct Box<T> {
    let value: T
}

func empty<T>() -> List<T> {
    return []
}

func count(items: List<Int>) -> Int {
    return items.count
}

func calls() -> Int {
    let a = empty<Int, String>()
    let b = Box<Int, Int>(value: 1)
    let c = count<Int>(items: [1])
    let d: Box<Int, Int>? = nil
    return c
}
