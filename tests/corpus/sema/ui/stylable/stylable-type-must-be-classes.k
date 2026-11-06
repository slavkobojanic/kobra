// A `@Stylable` prop is a class hook: its type is `Classes` (K3005 otherwise).
component Chip {
    @Stylable let container: String

    view {
        text { "chip" }
    }
}
