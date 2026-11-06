// An instance member is not reachable through the type: K1203.
struct Point {
    let x: Int

    func norm() -> Int {
        return x
    }
}

let n = Point.norm()
let x = Point.x
