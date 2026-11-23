public component Stepper {
    @Binding var value: Int

    view {
        Text("value")
    }
}

preview (value: Int = 1) {
    Stepper(value: bind value)
}

preview "at max" (value: Int = 99) {
    Stepper(value: bind value)
}

func previewed(preview: Int) -> Int {
    return preview
}
