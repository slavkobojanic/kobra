// `contains`, `startsWith` and `endsWith`; the empty string is contained in every string.
func main() -> List<Bool> {
    let word = "hello"
    let empty = ""
    return [word.contains("ell"), word.contains(""), word.startsWith("he"), word.startsWith("lo"), word.endsWith("lo"), empty.contains("a")]
}
