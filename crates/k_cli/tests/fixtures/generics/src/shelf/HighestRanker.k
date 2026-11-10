/// The `Ranker` the app injects: the highest score wins, the first on a
/// tie.
public service HighestRanker implements Ranker {
    func best<T>(_ items: List<T>, by score: (T) -> Int) -> T? {
        var top: T? = nil
        var topScore = 0
        for item in items {
            let s = score(item)
            if top == nil or s > topScore {
                top = item
                topScore = s
            }
        }
        return top
    }
}
