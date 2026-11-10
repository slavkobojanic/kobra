/// Picks the best of a list of anything by a score.
public interface Ranker {
    func best<T>(_ items: List<T>, by score: (T) -> Int) -> T?
}
