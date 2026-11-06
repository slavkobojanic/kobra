// `if let` in a view binds like in code.
component Card {
    @Prop let subtitle: String? = nil

    view {
        if let subtitle {
            paragraph { subtitle }
            //          ^^^^^^^^ resolves: local subtitle 6:16
        }
    }
}
