// `list` and `numberedList` hold `item`s, also inside `if` and `for`.
component Todo {
    @State var names: List<String> = []
    @State var showMore: Bool = false

    view {
        list {
            item { "first" }

            for name in names key name {
                item { name }
            }

            if showMore {
                item { "more" }
            }
        }
        numberedList {
            item { "1" }
        }
    }
}
