// `Equatable` needs every field to be `Equatable`.
struct Plain {
    let x: Int
}

struct Pair implements Equatable {
    let left: Plain
    let right: Int
}
