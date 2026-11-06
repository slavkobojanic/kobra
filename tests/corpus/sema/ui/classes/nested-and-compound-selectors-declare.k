// Every class named in a selector of the block is declared: nested ones (`&.active`),
// descendants (`.list .entry`) and classes inside `@media`.
component Menu {
    @State var isActive: Bool = false

    view {
        box {
            text { "a" }
                .class(.entry)
        }
        .class([.list, .active when isActive, .compact])
    }

    style {
        .list {
            padding: 4px;

            &.active { color: #FF0000; }

            &:hover { color: #00FF00; }
        }

        .list .entry { margin: 2px; }

        @media (maxWidth: 600px) {
            .compact { padding: 0; }
        }
    }
}
