// `@Prop` requires `let`: a `var` prop gets K3001 with a machine-applicable swap
// to `let`. The `let` prop next to it is fine.
component Badge {
    @Prop var label: String
    @Prop let count: Int

    view {
        text { "{label}: {count}" }
    }
}
