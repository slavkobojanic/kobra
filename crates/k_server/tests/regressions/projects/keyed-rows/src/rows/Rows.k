/// A keyed list whose row is an `if` block, so a row's nodes change while
/// the row lives.
public component Rows {
    @State var items: List<Row> = [Row(id: 1, isOn: true), Row(id: 2, isOn: false), Row(id: 3, isOn: true)]

    func toggle() {
        items = [Row(id: 1, isOn: true), Row(id: 2, isOn: true), Row(id: 3, isOn: true)]
    }

    func reverse() {
        items = [Row(id: 3, isOn: true), Row(id: 2, isOn: true), Row(id: 1, isOn: true)]
    }

    func swap() {
        items = [Row(id: 3, isOn: true), Row(id: 2, isOn: false), Row(id: 1, isOn: true)]
    }

    func clear() {
        items = []
    }

    view {
        box {
            button { "toggle" }
                .on(click: toggle)
            button { "reverse" }
                .on(click: reverse)
            button { "clear" }
                .on(click: clear)
            button { "swap" }
                .on(click: swap)
            box {
                for row in items key row.id {
                    if row.isOn {
                        text { "[{row.id}]" }
                    }
                }
            }
        }
    }
}
