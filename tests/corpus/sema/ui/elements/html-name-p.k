// `p` → `paragraph`; the children of an unknown element are still checked.
component Intro {
    view {
        p {
            text { "x" }
            span { "y" }
        }
    }
}
