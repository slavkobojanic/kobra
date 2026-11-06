// Set elements must be `Hashable`: a struct without it is K2009; with it, fine.
struct Point {
    let x: Int
}

struct Key implements Hashable {
    let x: Int
}

let points: Set<Point> = []
let keys: Set<Key> = [Key(x: 1)]
