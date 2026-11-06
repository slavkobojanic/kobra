// Clause order: generics, `implements`, `where`, body.
struct Box<T> implements Equatable where T implements Equatable {
    let value: T
}
