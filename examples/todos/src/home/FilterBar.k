import todo.Filter

/// Buttons that choose which todos the list shows.
///
/// `selection` is a `@Binding`: the bar writes it, and the parent's state
/// changes. The parent opts in at the call site with
/// `FilterBar(selection: bind filter)`, so the two-way data flow is visible
/// where it happens. Each button reports whether it is selected through
/// `pressed`, so assistive technology announces the current filter.
component FilterBar {
    /// The selected filter, shared with the parent.
    @Binding var selection: Filter

    private let filters: List<Filter> = [.all, .active, .completed]

    func displayName(filter: Filter) -> String {
        return match filter {
            case .all { "All" }
            case .active { "Active" }
            case .completed { "Completed" }
        }
    }

    view {
        nav {
            for filter in filters key filter {
                button { displayName(filter: filter) }
                    .pressed(filter == selection)
                    .class([.option, .selected when filter == selection])
                    .on(click: { selection = filter })
            }
        }
        .label("Filter todos")
        .class(.bar)
    }

    style {
        .bar {
            display: flex;
            gap: .xs;
        }

        .option {
            padding: .xs .sm;
            border: 1px solid transparent;
            borderRadius: .sm;
            backgroundColor: transparent;
            color: inherit;

            &:hover { borderColor: .border; }
        }

        .selected { borderColor: .primary; }
    }
}
