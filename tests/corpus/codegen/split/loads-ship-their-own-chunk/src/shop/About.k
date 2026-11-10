/// A page without a load: its chunk doesn't import the `load` chunk.
public page About at "/about" {
    view {
        heading(level: 1) { "About" }
    }
}
