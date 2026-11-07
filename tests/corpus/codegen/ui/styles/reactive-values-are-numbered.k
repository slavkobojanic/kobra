// Each interpolation in a style block gets the next number, counted from 0 in source order
// (codegen.md §12).
component Bar {
    @Prop let fill: Color = #00FF00
    @Prop let edge: Color = #000000

    view {
        box { "x" }
            .class(.bar)
    }

    style {
        .bar {
            backgroundColor: {fill};
            borderColor: {edge};
        }
    }
}
