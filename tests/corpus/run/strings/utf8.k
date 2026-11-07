// `utf8` is the UTF-8 encoding; multi-byte characters count every byte.
func main() -> List<Int> {
    return ["abc".utf8.count, "é".utf8.count, "😀".utf8.count, "".utf8.count]
}
