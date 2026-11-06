// A custom Binding built from get/set lambdas, passed as an argument.
component T {
    view {
        Slider(value: Binding(get: { celsius * 9 / 5 + 32 }, set: { f => celsius = (f - 32) * 5 / 9 }))
    }
}
