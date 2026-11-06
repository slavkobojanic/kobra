// A member of the enclosing type is found before a top-level declaration of the same name.
let limit = 10

struct Window {
    let limit: Int

    func isFull(size: Int) -> Bool {
        return size >= limit
        //             ^^^^^ resolves: member corpus.Window.limit
    }
}

func isFull(size: Int) -> Bool {
    return size >= limit
    //             ^^^^^ resolves: decl corpus.limit
}
