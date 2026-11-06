// Reading a field the struct doesn't have is K1203 at the member name, and the read's type is `{error}`.
struct Point {
    let x: Int
}

let p = Point(x: 1)
let z = p.z
        //^ resolves: none
