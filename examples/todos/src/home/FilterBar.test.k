import todo.Filter

test FilterBar {
    test "the bound filter starts out pressed" {
        let screen = render(FilterBarHost())

        expect screen.button(named: "All").isPressed
        expect screen.button(named: "Active").isPressed == false
        expect screen.button(named: "Completed").isPressed == false
    }

    test "clicking a filter selects it in the parent and releases the others" {
        let screen = render(FilterBarHost())

        screen.button(named: "Active").click()

        expect screen.text("Showing active").isVisible
        expect screen.button(named: "Active").isPressed
        expect screen.button(named: "All").isPressed == false
    }

    test "the bar writes through any binding, not only bound state" {
        var selected: Filter = .all
        let screen = render(FilterBar(selection: Binding(get: { selected }, set: { filter => selected = filter })))

        screen.button(named: "Completed").click()

        expect selected == .completed
    }

    test "the buttons sit in a labelled navigation landmark" {
        let screen = render(FilterBarHost())

        expect screen.navigation(named: "Filter todos").buttons().count == 3
    }
}

/// Hosts a `FilterBar` the way a page does, with `@State` passed by `bind`,
/// and shows the selection so tests can see the parent's state change.
component FilterBarHost {
    @State var filter: Filter = .all

    @Derived let summary: String = match filter {
        case .all { "Showing all" }
        case .active { "Showing active" }
        case .completed { "Showing completed" }
    }

    view {
        FilterBar(selection: bind filter)

        paragraph { summary }
    }
}
