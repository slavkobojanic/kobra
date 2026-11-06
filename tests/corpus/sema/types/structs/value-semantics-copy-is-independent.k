// Structs are values: a `var` copy can be changed without touching the original, and both keep their type.
struct Counter {
    var count: Int
}

func bumped() -> Int {
    let original = Counter(count: 1)
    var copy = original
    copy.count = 2
  //^^^^ resolves: local copy 8:9
    return original.count
                  //^^^^^ resolves: member corpus.Counter.count
}
