// When every field has a default, the memberwise initializer takes no arguments at all.
struct Options {
    var dense: Bool = false
    var columns: Int = 2
}

let options = Options()
  //^^^^^^^ type: corpus.Options
