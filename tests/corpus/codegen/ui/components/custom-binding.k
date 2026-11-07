// `Binding(get:set:)` is the same `{ get, set }` pair built from lambdas (04-components.md).
component Stepper {
    @Binding var value: Int

    view {
        text { "{value}" }
    }
}

component Thermometer {
    @State var celsius: Int = 20

    view {
        Stepper(value: Binding(get: { celsius * 9 / 5 + 32 }, set: { f => celsius = (f - 32) * 5 / 9 }))
    }
}
