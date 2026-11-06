// For a class, `let` fixes the reference only: its `var` fields can still be assigned.
class Counter {
    var count: Int = 0
}

func tick() -> Int {
    let counter = Counter()
    counter.count = 1
    counter.count += 1
    return counter.count
                 //^^^^^ resolves: member corpus.Counter.count
}
