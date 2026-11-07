// `list`/`item`, `numberedList`, `definitionList`/`term`/`definition` and `table`/`row`/
// `headerCell`/`cell` map to their HTML tags. A table's rows are wrapped in `tbody`, which the HTML
// parser would insert anyway, so template paths match the parsed tree.
component Specs {
    view {
        section {
            list {
                item { "one" }
            }

            numberedList {
                item { "first" }
            }

            definitionList {
                term { "K" }

                definition { "a language" }
            }

            table {
                row {
                    headerCell { "Name" }

                    cell { "Lamp" }
                }
            }
        }
    }
}
