// A view that calls one of the component's own methods runs that method
// while it renders, so the method's element-reference operations are K3116
// at the view's call. One level is followed: `deeper()` reaches `focus()`
// only through `focusName()`, and isn't reported. A handler may call
// the operations, and a lambda inside the method runs later.
import k.ui.ElementRef

component Search {
    let name = ElementRef()

    func focusName() -> Bool {
        name.focus()
        return true
    }

    func scrolled() -> Bool {
        self.name.scrollIntoView()
        return true
    }

    func deeper() -> Bool {
        return focusName()
    }

    func later() -> Bool {
        let f = { name.blur() }
        return true
    }

    view {
        if focusName() {
            text { "Focused" }
        }
        if self.scrolled() and deeper() and later() {
            text { "Scrolled" }
        }
        button { "Name" }
            .ref(name)
            .on(click: { name.focus() })
    }
}
