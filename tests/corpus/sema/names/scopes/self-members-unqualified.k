// Inside a type, its members resolve unqualified; `self.` is optional.
struct Counter {
    let count: Int
    let step: Int

    func next() -> Int {
        return count + self.step
        //     ^^^^^ resolves: member corpus.Counter.count
        //                  ^^^^ resolves: member corpus.Counter.step
    }

    func twice() -> Int {
        return next() + next()
        //     ^^^^ resolves: member corpus.Counter.next()
    }
}
