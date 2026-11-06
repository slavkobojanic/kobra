// @media nested inside a rule, and another @media nested inside it.
component T {
    style {
        .a {
            @media (maxWidth: 600px) {
                padding: .sm;

                @media (orientation: portrait) {
                    padding: .xs;
                }
            }
        }
    }
}
