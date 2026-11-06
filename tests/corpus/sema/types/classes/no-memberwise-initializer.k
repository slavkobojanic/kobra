// Classes have no memberwise initializer: with all fields defaulted they are built with `()`, and labels are unknown.
class Cache {
    var size: Int = 0
}

let empty = Cache()
  //^^^^^ type: corpus.Cache
let sized = Cache(size: 4)
