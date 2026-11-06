// K2008 offers a maybe-incorrect fix inserting the missing labels with
// placeholders (diagnostics.md, types).
struct Point {
    let x: Int
    let y: Int
    let z: Int = 0
}

let p = Point(x: 1)
