// Stylable class hooks passed with conditional classes (from the styles design).
component T {
    view {
        CartRow(item: item)
            .container(.highlighted when item.id == selectedId)
            .title(.strikethrough when item.isRemoved)
    }
}
