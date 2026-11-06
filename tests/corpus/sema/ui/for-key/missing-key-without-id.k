// Without an `id` on the item type there is nothing to suggest.
component Names {
    @Prop let names: List<String>

    view {
        for name in names {
            text { name }
        }
    }
}
