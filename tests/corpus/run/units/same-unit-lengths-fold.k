// Same-unit length arithmetic stays one length.
func main() -> List<String> {
    let gutter = 16px * 2
    let twice = 2 * 8px
    let sum = 8px + 8px
    let third = 30px / 3
    let negative = -4px
    let rem = 1.5rem + 0.5rem
    return ["{gutter}", "{twice}", "{sum}", "{third}", "{negative}", "{rem}"]
}
