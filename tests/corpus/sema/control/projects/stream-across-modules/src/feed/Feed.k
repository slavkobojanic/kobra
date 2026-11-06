/// A source of updates.
public struct Feed {
    let source: Stream<Int>

    func changes() -> Stream<Int> {
        return source
    }
}
