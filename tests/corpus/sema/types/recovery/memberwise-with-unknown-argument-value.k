// An argument that fails to resolve still fills its label: no K2008 for that field, no K2001.
struct Point {
    let x: Int
    let y: Int
}

let p = Point(x: 1, y: nowhere)
