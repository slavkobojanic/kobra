// A reference names one element: attached once, never in a `for` body or a scoped slot (K3114),
// and never left unattached (K3115).
import k.ui.ElementRef

component Rows {
    @Prop let names: List<String>

    let row = ElementRef()
    let title = ElementRef()
    let unused = ElementRef()

    view {
        heading(level: 2) { "Rows" }
            .ref(title)
        paragraph { "Again" }
            .ref(title)
        list {
            for name in names key name {
                item { name }
                    .ref(row)
            }
        }
    }
}
