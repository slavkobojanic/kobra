// `Page` is generated for the app and needs no import, whichever module the page lives in.
public page Home {
    view {
        link(to: .product(id: 1)) { "Lamp" }
        //        ^^^^^^^ resolves: member Page.product
    }
}
