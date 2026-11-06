// In a view, members, `let` constants and `for` variables resolve like in any block.
struct Item {
    let id: Int
    let name: String
}

component ItemList {
    @Prop let items: List<Item>
    @State var highlighted = 0

    view {
        let heading = "Items"
        paragraph { heading }
        //          ^^^^^^^ resolves: local heading 12:13
        for item in items key item.id {
        //          ^^^^^ resolves: member corpus.ItemList.items
            paragraph { "{item.name} {highlighted}" }
            //            ^^^^ resolves: local item 15:13
            //                 ^^^^ resolves: member corpus.Item.name
            //                        ^^^^^^^^^^^ resolves: member corpus.ItemList.highlighted
        }
    }
}
