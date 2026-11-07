// `split` keeps empty parts; `limit` caps the number of parts and the last one holds the rest.
func main() -> List<List<String>> {
    let text = "a,b,,c"
    return [text.split(separator: ","), text.split(separator: ",", limit: 2), "abc".split(separator: ","), "a--b".split(separator: "--")]
}
