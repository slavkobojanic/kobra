// A private top-level declaration is visible everywhere in its own file.
private let limit = 3

private struct Window {
    let size: Int
}

private func clamp(size: Int) -> Window {
    if size > limit {
    //        ^^^^^ resolves: decl corpus.limit
        return Window(size: limit)
    }
    return Window(size: size)
    //     ^^^^^^ resolves: decl corpus.Window
}
