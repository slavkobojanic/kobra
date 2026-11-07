// `truncated` cuts to at most `length` characters including the ending, and leaves short strings alone.
func main() -> List<String> {
    let long = "Hello world"
    return [long.truncated(length: 8, ending: "…"), "Hello".truncated(length: 8), long.truncated(length: 8), long.truncated(length: 6, ending: "...")]
}
