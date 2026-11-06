// Apps add their own number extensions with `extend Int`.
struct Percentage {
    let value: Int
}

extend Int {
    var percent: Percentage { Percentage(value: self) }
}

let tax = 25.percent
  //^^^ type: corpus.Percentage
