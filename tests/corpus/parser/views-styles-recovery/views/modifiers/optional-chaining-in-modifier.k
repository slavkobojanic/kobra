// Optional member access and `??` inside a modifier argument.
component T {
    view {
        button { "Save" }
            .disabled(form?.isBusy ?? false)
    }
}
