// Defaults are evaluated in order and can use earlier parameters; a middle argument can be omitted.
func box(width: Int, height: Int = 0, label: String = "box") -> String {
    let h = if height == 0 { width } else { height }
    return "{label} {width}x{h}"
}

func main() -> List<String> {
    return [box(width: 2), box(width: 2, label: "sq"), box(width: 2, height: 3), box(width: 1, height: 1, label: "unit")]
}
