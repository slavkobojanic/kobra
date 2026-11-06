// A capitalized name is a component, not an element: an unknown one is K1201
// (name resolution), not K3101. Its arguments, chained options and events cause
// nothing more.
component Home {
    view {
        Cardd(title: "x")
            .size(.large)
            .on(click: { })
    }
}
