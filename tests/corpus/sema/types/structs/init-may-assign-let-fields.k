// Inside an initializer, `let` fields are assigned once; that is not K2011.
struct Span {
    let low: Int
    let high: Int

    init(around center: Int) {
        low = center - 1
        high = center + 1
    }
}

let r = Span(around: 5)
  //^ type: corpus.Span
