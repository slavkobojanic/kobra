// Previews of a component: typed parameters with defaults are the
// preview's state, so a `@Binding` takes `bind value`.
enum Size {
    case small
    case large
}

component Stepper {
    @Binding var value: Int
    @Prop let step: Int = 1
    @Prop let label: String = "Count"
    @Prop let size: Size = .small

    view {
        "{label}: {value}"
    }
}

preview (value: Int = 1, step: Int = 1, label: String = "Items", size: Size = .large) {
    Stepper(value: bind value, step: step, label: label, size: size)
}

preview "at max" (value: Int = 99, ratio: Double = 0.5, on: Bool = true, wait: Duration = 2.seconds) {
    let doubled = value * 2
    Stepper(value: bind value, step: doubled)
}
