// `Icon` is a `span` with the icon's name in `data-icon`, also when the name is computed
// (bound like any attribute, as the case's name), and the stylesheet draws each icon of the
// bundled set as a mask in the current text colour, 1em square, so an icon is visible and takes
// the size and colour of the text around it (20-standard-library.md, `k.ui.Icon`; codegen.md §10).
import k.ui.Icon
import k.ui.IconName

component Status {
    @Prop let sunny: Bool

    @Derived let symbol: IconName = if sunny { .sun } else { .cloudRain }

    view {
        Icon(name: symbol)
        Icon(name: .arrowUp, label: "Wind")
    }
}
