// References between declarations of one module are never a cycle error, even when mutual.
struct Author {
    let latest: Book?
    //          ^^^^ resolves: decl corpus.Book
}

struct Book {
    let author: Author?
    //          ^^^^^^ resolves: decl corpus.Author
}
