// `bind` outside an argument is K0110; the BindExpr is still built.
component T {
    view {
        text { bind name }
    }
}
