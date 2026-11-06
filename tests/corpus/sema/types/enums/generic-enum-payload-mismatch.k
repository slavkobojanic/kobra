// With the enum's type argument fixed by the annotation, a wrong payload is K2001.
enum Fetch<T> {
    case idle
    case loaded(T)
}

let wrong: Fetch<Int> = .loaded("five")
