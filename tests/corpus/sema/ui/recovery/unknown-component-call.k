// An unknown component is K1201; its `bind` argument, chained options, events
// and trailing block are not checked against anything, but the trailing block's
// own content still is (the HTML name inside is K3101).
component Cart {
    @State var quantity: Int = 1

    view {
        Steper(value: bind quantity)
            .step(2)
            .on(changed: { v => quantity = v })
        Panel {
            div { "x" }
        }
    }
}
