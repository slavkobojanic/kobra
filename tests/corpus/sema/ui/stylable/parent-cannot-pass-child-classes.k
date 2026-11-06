// The classes a parent passes resolve against the parent's style block: the
// child's `.row` is not one of them (K3203).
component CartRow {
    @Stylable let container: Classes

    view {
        box { "row" }
            .class([.row, container])
    }

    style {
        .row { padding: 4px; }
    }
}

component Cart {
    view {
        CartRow()
            .container(.row)
        CartRow()
            .container(.selected)
    }

    style {
        .selected { color: #000000; }
    }
}
