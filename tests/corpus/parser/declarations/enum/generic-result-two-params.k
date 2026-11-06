// Two generic parameters and a `where` clause.
public enum Result<T, E> where E implements Error {
    case success(T)
    case failure(E)
}
