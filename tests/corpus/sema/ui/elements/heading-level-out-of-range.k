// The heading level is an integer literal from 1 to 6 (element table): `7`, `0`
// and a variable are K3109; `6` is fine.
component Title {
    @Prop let level: Int

    view {
        heading(level: 7) { "a" }
        heading(level: 0) { "b" }
        heading(level: level) { "c" }
        heading(level: 6) { "d" }
    }
}
