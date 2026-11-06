// A generic enum with unlabeled associated values.
enum LoadState<T> {
    case idle
    case loading
    case loaded(T)
    case failed(Error)
}
