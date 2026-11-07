// A payload-free enum case interpolates as its name.
enum Size {
    case small
    case extraLarge
}

func main() -> List<String> {
    let s: Size = .small
    let l = Size.extraLarge
    return ["{s}", "size: {l}"]
}
