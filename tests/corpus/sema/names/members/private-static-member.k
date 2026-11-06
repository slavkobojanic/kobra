// Private static members are hidden like instance members; the public neighbour resolves.
struct Limits {
    private static let hidden: Int = 7
    static let shown: Int = 3
}

func both() -> Int {
    return Limits.shown + Limits.hidden
    //            ^^^^^ resolves: member corpus.Limits.shown
}
