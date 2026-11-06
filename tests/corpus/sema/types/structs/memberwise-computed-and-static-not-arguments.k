// Computed properties and static members are not stored fields, so they are not memberwise arguments.
struct Rect {
    let width: Int
    let height: Int
    var area: Int { width * height }
    static let unit = Rect(width: 1, height: 1)
}

let r = Rect(width: 2, height: 3)
let a = r.area
  //^ type: Int
        //^^^^ resolves: member corpus.Rect.area
