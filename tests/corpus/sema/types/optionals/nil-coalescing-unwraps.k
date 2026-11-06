// `a ?? b` has the unwrapped type when `b` is not optional.
func display(name: String?) -> String {
    let shown = name ?? "anonymous"
      //^^^^^ type: String
    return shown
}
