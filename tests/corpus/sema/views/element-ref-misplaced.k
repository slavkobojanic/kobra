// An element reference is a plain `let` field of the component whose element it names, made with
// `ElementRef()`; `.ref(…)` takes such a field of its own declaration (K3113).
import k.ui.ElementRef

service Focus {
    let target = ElementRef()
}

component Child {
    @Prop let given: ElementRef
    @State var held: ElementRef = ElementRef()
    var changing = ElementRef()

    view {
        button { "Go" }
            .ref(given)
    }
}

component Parent {
    let mine = ElementRef()

    func later() {
        let local = ElementRef()
        local.focus()
    }

    view {
        Child(given: mine)
        box { "Here" }
            .ref(mine)
    }
}
