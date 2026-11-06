// A payload-less case of a generic enum without an expected type can't fix `T`: K2002.
enum Fetch<T> {
    case idle
    case loaded(T)
}

let known: Fetch<Int> = .idle
let unknown = Fetch.idle
