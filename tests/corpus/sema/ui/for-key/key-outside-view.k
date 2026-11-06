// `key` belongs to view loops; on a `for` in a function body it is K3111 with a
// fix that removes the clause (grammar.md §15).
component Totals {
    @Prop let values: List<Int>
    @State var total: Int = 0

    func sum() {
        for value in values key value {
            total += value
        }
        for value in values {
            total += value
        }
    }

    view {
        text { "{total}" }
    }
}
