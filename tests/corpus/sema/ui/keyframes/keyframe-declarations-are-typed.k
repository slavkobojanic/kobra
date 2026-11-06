// Declarations inside keyframes are checked like any other: unknown property
// (K3201) and wrong value (K3202).
component Pulse {
    view {
        box { "x" }
            .class(.pulse)
    }

    style {
        @keyframes pulse {
            from { opacty: 1; }
            to { opacity: red; }
        }

        .pulse { animationName: .pulse; }
    }
}
