// @container with a feature, @layer with a block and @layer as a statement ending in `;`.
component T {
    style {
        @container (minWidth: 400px) {
            .card {}
        }
        @layer base {}
        @layer reset;
    }
}
