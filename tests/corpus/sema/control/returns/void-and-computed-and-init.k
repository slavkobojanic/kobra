// `Void` functions, `init`s and computed properties need no `return`: a
// computed property's value is its last expression.
struct Box {
    let width: Int
    let height: Int

    var area: Int { width * height }

    init(side: Int) {
        width = side
        height = side
    }

    func log() {
        if area > 10 {
            return
        }
    }
}
