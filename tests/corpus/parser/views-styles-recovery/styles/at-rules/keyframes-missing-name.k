// A `@keyframes` without a name reports K0100 at the `{`; the blocks still parse.
component T {
    style {
        @keyframes {
            to { opacity: 1; }
        }
    }
}
