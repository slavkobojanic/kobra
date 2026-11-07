// Only lists of `Int` or `Double` have `sum()`: other element types are K2009, and the call still
// has the element type, so nothing cascades. `Decimal` and units need their own sums.
struct Item {
    let name: String
}

func f(names: List<String>, items: List<Item>, flags: List<Bool>, delays: List<Duration>) {
    let a = names.sum()
    let b = items.sum()
    let c = flags.sum()
    let d = delays.sum()
}
