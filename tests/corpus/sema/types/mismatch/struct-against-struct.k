// Two structs with the same fields are distinct, nominal types.
struct Meters {
    let value: Double
}

struct Feet {
    let value: Double
}

func walk(distance: Meters) {}

func run() {
    walk(distance: Feet(value: 3.0))
}
