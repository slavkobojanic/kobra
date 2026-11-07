// `+` concatenates strings; the empty string is the identity.
func main() -> List<String> {
    let first = "Ana"
    let last = "Lima"
    let empty = ""
    return [first + " " + last, empty + empty, first + "{last.count}", empty + first]
}
