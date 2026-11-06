// Not duplicates: overloaded functions (the stdlib uses them), a property
// sharing a name with a method (sema.md §16, round 1a), and an instance
// field next to a static member of the same name (`x.count` vs `S.count`).
struct S {
    let count: Int

    static let count: Int = 0

    func scale(by: Int) -> Int {
        return by
    }

    func scale(by: Double) -> Double {
        return by
    }

    func count() -> Int {
        return 1
    }
}
