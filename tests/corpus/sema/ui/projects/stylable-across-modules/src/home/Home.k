// The parent passes its own classes to a `@Stylable` prop of a component from
// another module; the child's own class `.chip` isn't among them (K3203).
import ui.Chip

public page Home {
    @State var isSelected: Bool = false

    view {
        Chip(text: "a")
            .container(.selected when isSelected)
            //          ^^^^^^^^ resolves: class home.Home.selected
        Chip(text: "b")
            .container(.chip)
    }

    style {
        .selected { fontWeight: bold; }
    }
}
