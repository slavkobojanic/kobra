// A struct converts to a project interface (and a concrete error to
// `Error`) by changing how it is held, which happens for a value itself,
// an optional's value and a list's elements; inside a `Set`, a map's values,
// a `Result` or a function's result it is K2001 with a help.
interface Priced {
    func price() -> Int
}

struct Card implements Priced, Hashable {
    let cents: Int

    func price() -> Int {
        return cents
    }
}

func convert(cards: Set<Card>, byName: Map<String, Card>, make: () -> Card) {
    let one: Priced = Card(cents: 1)
    let maybe: Priced? = Card(cents: 2)
    let set: Map<String, Priced> = byName
    let made: () -> Priced = make
}
