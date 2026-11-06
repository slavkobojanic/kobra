// A condition list mixing `let` bindings and a boolean condition.
component T {
    view {
        if let a, let b = other, a.isReady {
            Ready()
        }
    }
}
