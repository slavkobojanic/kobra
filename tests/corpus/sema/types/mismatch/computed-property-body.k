// A computed property's last expression must have the property's type.
struct Rect {
    let width: Int
    var label: Int { "w{width}" }
}
