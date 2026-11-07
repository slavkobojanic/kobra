// Constant attributes are written into the template: `.type(.submit)` → `type`,
// `.placeholder` → `placeholder`, `.role(.status)` → `role`, `.label` on a landmark → `aria-label`
// (05-views.md, "Attributes are per element").
component Search {
    @State var query: String = ""

    view {
        nav {
            textField(value: bind query)
                .placeholder("Search")

            paragraph { "3 results" }
                .role(.status)

            button { "Go" }
                .type(.submit)
        }
        .label("Main")
    }
}
