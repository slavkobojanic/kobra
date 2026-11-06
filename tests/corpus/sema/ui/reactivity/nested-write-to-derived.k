// Writing a field of a derived struct writes the derived value (K3010 at the
// place).
struct Point {
    var x: Int
}

component Plot {
    @State var offset: Int = 0
    @Derived let origin: Point = Point(x: offset)

    func nudge() {
        origin.x = 1
    }

    view {
        text { "{origin.x}" }
    }
}
