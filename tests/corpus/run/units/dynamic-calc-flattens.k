// A length computed at run time builds `calc()` text, and a calc inside a calc is flattened.
func widen(base: Length) -> Length {
    return base + 2rem
}

func main() -> List<String> {
    let once = widen(base: 10px)
    let twice = widen(base: once)
    return ["{once}", "{twice}"]
}
