// Chaining a name the component doesn't define is K3103; the argument isn't
// checked further, so `.red` causes no K2007.
component Badge {
    @Prop let text: String
    @Prop let tone: Int = 0

    view {
        paragraph { text }
    }
}

component Home {
    view {
        Badge(text: "New")
            .colour(.red)
            .tone(1)
    }
}
