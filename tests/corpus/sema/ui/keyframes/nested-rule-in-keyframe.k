// A keyframe block holds declarations only: a nested rule or at-rule is K3208.
component Pulse {
    view {
        box { "x" }
            .class(.pulse)
    }

    style {
        @keyframes pulse {
            from {
                opacity: 1;
                &:hover { opacity: 0; }
            }
            to {
                @media (maxWidth: 600px) { opacity: 0; }
            }
        }

        .pulse { animationName: .pulse; }
    }
}
