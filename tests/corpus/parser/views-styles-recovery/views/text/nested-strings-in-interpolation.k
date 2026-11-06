// An if expression with nested string literals inside an interpolation.
component T {
    view {
        text { "{if done { "Done" } else { "Pending" }}" }
    }
}
