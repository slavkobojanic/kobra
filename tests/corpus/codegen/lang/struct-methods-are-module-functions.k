// Struct methods and extension members are module functions taking `self`, so values stay plain
// objects (docs/compiler/codegen.md §9).
struct Point {
    let x: Int
    let y: Int

    func moved(by d: Int) -> Point {
        return Point(x: x + d, y: y + d)
    }
}

extend Int {
    var squared: Int { self * self }
}

func apply(p: Point) -> Int {
    return p.moved(by: 2).x.squared
}
