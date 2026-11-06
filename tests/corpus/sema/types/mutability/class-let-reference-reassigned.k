// Reassigning a `let` class reference is K2011.
class Counter {
    var count: Int = 0
}

func reset() -> Int {
    let counter = Counter()
    counter = Counter()
    return counter.count
}
