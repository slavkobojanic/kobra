// `table` holds `row`s; a `cell` directly in a table is K3107. Rows hold cells.
component Grid1 {
    view {
        table {
            row {
                headerCell { "Name" }
                cell { "Ada" }
            }
            cell { "stray" }
        }
    }
}
