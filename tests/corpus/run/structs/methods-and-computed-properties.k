// Struct methods read `self`'s fields; computed properties are recomputed on every read.
struct Rect {
    var width: Int
    var height: Int

    var area: Int { width * height }

    func scaled(by factor: Int) -> Rect {
        return Rect(width: width * factor, height: height * factor)
    }

    func fits(in other: Rect) -> Bool {
        return width <= other.width and height <= other.height
    }
}

func main() -> List<Int> {
    var r = Rect(width: 2, height: 3)
    let before = r.area
    r.width = 10
    let big = r.scaled(by: 2)
    let fits = if r.fits(in: big) { 1 } else { 0 }
    return [before, r.area, big.area, fits]
}
