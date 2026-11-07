// Same-unit lengths fold to one literal, mixed ones become `calc()`, and a dynamic operand builds the
// text at run time (docs/compiler/codegen.md §6.5).
func gutter() -> Length {
    return 16px * 2
}

func mixed() -> Length {
    return 100px + 2rem
}

func widen(base: Length) -> Length {
    return base + 2rem
}
