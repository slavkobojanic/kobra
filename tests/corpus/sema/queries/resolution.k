struct Point {
    let x: Int
}

func shift(point: Point, by: Int) -> Int {
    let moved = point.x + by
    //          ^^^^^ resolves: param point 5:12
    //                ^ resolves: member corpus.Point.x
    //                    ^^ resolves: param by 5:26
    return moved
    //     ^^^^^ resolves: local moved 6:9
}
//@ 5:19-5:24 resolves: decl corpus.Point
//@ 5:38-5:41 resolves: decl Int
