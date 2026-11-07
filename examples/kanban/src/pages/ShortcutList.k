/// The board page's keyboard shortcuts, and how to move a card with the
/// keyboard.
component ShortcutList {
    view {
        section {
            heading(level: 2) { "Keyboard shortcuts" }

            definitionList {
                term { "N" }

                definition { "Add a card to the first column" }

                term { "E" }

                definition { "Export the board" }

                term { "?" }

                definition { "Show or hide this list" }

                term { "Escape" }

                definition { "Close the editor" }

                term { "Space or Enter on a card" }

                definition { "Pick it up; then the arrow keys move it, Space or Enter drops it and Escape puts it back" }
            }
        }
        .label("Keyboard shortcuts")
        .class(.shortcuts)
    }

    style {
        .shortcuts {
            padding: .md;
            borderRadius: .md;
            backgroundColor: .surface;
            border: 1px solid .border;
        }
    }
}
