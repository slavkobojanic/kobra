// Only the required prop that is missing (`city`) is reported, at the insertion
// point after the last argument.
component Address {
    @Prop let street: String
    @Prop let city: String

    view {
        text { "{street}, {city}" }
    }
}

component Home {
    view {
        Address(street: "Main")
    }
}
