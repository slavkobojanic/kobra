// `{ }` holds reactive values (props and state); their type must be one of the
// property's K types.
component Summary {
    @Prop let accent: Color = #3B82F6
    @State var gap: Length = 8px
    @Derived let half: Length = 4px

    view {
        box { "x" }
            .class(.summary)
    }

    style {
        .summary {
            borderLeft: 4px solid {accent};
            //                     ^^^^^^ resolves: member corpus.Summary.accent
            //                     ^^^^^^ type: Color
            padding: {gap} {half};
        }
    }
}
