component Names {
    @Prop let names: List<String>

    view {
        for name in names {
            text { name }
        }
        for name in names key name {
            text { name }
        }
    }
}
