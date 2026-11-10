// An element reference's operations run after rendering: a view or a field initializer can't call
// them (K3116); a handler can.
import k.ui.ElementRef

component Panel {
    let box = ElementRef()

    @Derived let shown: Bool = moved(box.focus())

    func moved(_ nothing: Void) -> Bool {
        return true
    }

    view {
        if moved(box.scrollIntoView()) {
            text { "Moved" }
        }
        button { "Body" }
            .ref(box)
            .on(click: { box.focus() })
    }
}
