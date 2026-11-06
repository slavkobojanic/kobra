// Contextual keywords are ordinary names after the declaration keyword and as field names.
component view {
    @Prop let page: Int
    @Prop let key: String
    @Prop let test: Bool
    @Prop let variant: Style
    @Prop let at: Int
    func layout() {}
}
