// The exact stylesheet of a small component: readable class names under the test profile, one
// declaration per line indented by two spaces, rules in source order, kebab-case properties
// (codegen.md §12, "Printing").
component Badge {
    @State var isNew: Bool = true

    view {
        text { "new" }
            .class([.pill, .fresh when isNew])
    }

    style {
        .pill {
            padding: 4px;
            borderRadius: 8px;
        }

        .fresh { backgroundColor: #FEF3C7; }
    }
}
