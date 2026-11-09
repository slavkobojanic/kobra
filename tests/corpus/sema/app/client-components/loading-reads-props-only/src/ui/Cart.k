let rows: Int = 2

func label(_ n: Int) -> String {
    return "{n} rows"
}

@Rendering(.client)
public component Cart {
    @Prop let owner: String

    @Slot let empty: View

    @State var count: Int = 0

    @Derived let doubled: Int = count * 2

    func summary() -> String {
        return "{count} items"
    }

    view {
        text { "{owner}: {count}" }
    }

    loading {
        text { "{owner}, {label(rows)}" }
        text { "{count}" }
        text { "{self.doubled}" }
        text { summary() }
        text { self.summary() }
        empty
    }
}
