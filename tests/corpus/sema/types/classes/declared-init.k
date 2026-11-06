// A class is constructed through its declared `init`.
class Cache {
    let capacity: Int
    var size: Int = 0

    init(capacity: Int) {
        self.capacity = capacity
    }
}

let cache = Cache(capacity: 8)
  //^^^^^ type: corpus.Cache
          //^^^^^ resolves: decl corpus.Cache
