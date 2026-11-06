// A helper component of another module can't be imported (K1102); the call of
// it reports nothing more.
import ui.Button
import ui.ButtonIcon

public page Home {
    view {
        Button(title: "Go")
        ButtonIcon()
    }
}
