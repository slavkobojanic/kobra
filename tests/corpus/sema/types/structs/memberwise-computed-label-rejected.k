// A computed property can't be passed to the memberwise initializer.
struct Rect {
    let width: Int
    let height: Int
    var area: Int { width * height }
}

let r = Rect(width: 2, height: 3, area: 6)
