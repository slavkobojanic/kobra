/// One generic `Picker<T>` placed twice: for fruits, its type argument
/// inferred from the options, and for quantities, written out. Picking
/// keeps the choice in the `Basket` service through its generic members,
/// and the injected `Ranker` names the dearest fruit.
page Home at "/" {
    @Inject let basket: Basket
    @Inject let ranker: Ranker

    @State var fruit: String = "none"
    @State var quantity: Int = 0

    let fruits: List<Fruit> = [
        Fruit(name: "Apple", price: 3),
        Fruit(name: "Mango", price: 9),
        Fruit(name: "Pear", price: 4),
    ]

    func choose(_ picked: Fruit) {
        basket.keep(picked)
        let kept = basket.last<Fruit>()
        fruit = kept?.name ?? "none"
    }

    func count(_ picked: Int) {
        basket.keep<Int>(picked)
        quantity = basket.last<Int>() ?? 0
    }

    view {
        heading(level: 1) { "Generics" }

        Picker(title: "Fruit", options: fruits, label: { f => f.name })
            .on(picked: choose)

        Picker<Int>(title: "Quantity", options: [1, 2, 3], label: { n => "{n} pieces" })
            .on(picked: count)

        text { "Picked {fruit} × {quantity}" }
        text { "Dearest: {ranker.best<Fruit>(fruits, by: { f => f.price })?.name ?? "none"}" }
        text { "Kept {basket.saved.count}" }
    }
}
