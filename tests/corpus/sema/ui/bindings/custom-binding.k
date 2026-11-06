// A custom `Binding(get:set:)` is passed without `bind`; its type is the
// binding type the prop expects.
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
        //             ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ type: Binding<Int>
    }
}
