// `dateField` binds a `Date?`: a `@State var` of type `Date?` binds directly.
component Booking {
    @State var day: Date? = nil

    view {
        dateField(value: bind day)
            .label("Day")
    }
}
