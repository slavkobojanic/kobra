// K2205 applies to methods too: a struct method that isn't `async` can't await.
struct Counter {
    let start: Int

    func fetch() async -> Int {
        return start
    }

    func next() -> Int {
        return await fetch() + 1
    }

    func nextAsync() async -> Int {
        return await fetch() + 1
    }
}
