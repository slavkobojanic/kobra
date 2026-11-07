// `replace` replaces every occurrence; `prefix` and `repeated` handle out-of-range counts.
func main() -> List<String> {
    return ["a-b-c".replace("-", with: "+"), "aaa".replace("a", with: "bb"), "hello".prefix(2), "hi".prefix(10), "ab".repeated(times: 3), "x".repeated(times: 0)]
}
