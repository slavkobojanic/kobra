// Several selectors share a block; blocks and the rule itself may be empty.
component T {
    style {
        @keyframes pulse {
            from, to { opacity: 1; }
            25%, 75% {}
        }
        @keyframes nothing {}
    }
}
