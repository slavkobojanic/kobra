// Each view `for` needs its own key, nested or inside `if`.
component Matrix {
    @Prop let rows: List<List<String>>
    @State var show: Bool = true

    view {
        for row in rows key row.joined(separator: ",") {
            for cell in row {
                text { cell }
            }
        }

        if show {
            for row in rows {
                text { "row" }
            }
        }
    }
}
