// A chained option of a function type takes a trailing block, as a slot does: `.format { n => … }`
// passes the lambda as the prop's value (04-components.md, chained options).
component Counter {
    @Prop let count: Int
    @Prop let format: (Int) -> String = { n => "{n}" }

    view {
        text { format(count) }
    }
}

component Page {
    view {
        Counter(count: 3)
            .format { n => "#{n} items" }
    }
}
