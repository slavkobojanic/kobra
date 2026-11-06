// In an `init`, parameters shadow fields of the same name; `self.` reaches the fields.
struct Span {
    let start: Int
    let end: Int

    init(start: Int, length: Int) {
        self.start = start
        //   ^^^^^ resolves: member corpus.Span.start
        //           ^^^^^ resolves: param start 6:10
        self.end = start + length
        //   ^^^ resolves: member corpus.Span.end
        //                 ^^^^^^ resolves: param length 6:22
    }
}
