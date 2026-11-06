// CSS functions with operators, comma-separated arguments, nesting and several functions in one value.
component T {
    style {
        .a {
            width: calc(100% - 2px);
            background: rgba(0, 0, 0, 0.5);
            transform: translate(-50%, -50%) rotate(45deg);
            gridTemplateColumns: repeat(3, 1fr);
            height: calc(100% - calc(2px * 3));
        }
    }
}
