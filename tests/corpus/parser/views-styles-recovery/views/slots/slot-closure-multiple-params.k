// A slot closure with two unparenthesized parameters.
component T {
    view {
        Table(rows: rows)
            .cell { row, column =>
                text { "{row}:{column}" }
            }
    }
}
