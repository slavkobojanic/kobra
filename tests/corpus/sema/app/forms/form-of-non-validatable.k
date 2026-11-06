// `@Form` wraps a `Validatable` struct; the constraint is reported once, at the field's type.
struct Point {
    var x: Int
}

page Plot {
    @Form var point: Point = .empty

    view {
        text { "plot" }
    }
}
