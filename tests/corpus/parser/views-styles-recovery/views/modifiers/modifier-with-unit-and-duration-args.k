// Modifier arguments that are unit literals and member calls on numbers.
component T {
    view {
        box {}
            .animate(.fade, duration: 200ms, delay: 2.seconds)
    }
}
