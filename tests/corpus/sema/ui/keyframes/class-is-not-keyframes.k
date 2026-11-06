// Classes and keyframes are separate: `animationName: .toast` names a class, not
// keyframes (K3205).
component Toast {
    view {
        box { "x" }
            .class(.toast)
    }

    style {
        .toast { animationName: .toast; }
    }
}
