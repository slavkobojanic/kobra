// Strings compare by UTF-16 code units: upper case sorts before lower case, a prefix before its extensions.
func main() -> List<Bool> {
    let empty = ""
    return ["apple" < "banana", "Zebra" < "apple", "abc" == "abc", "a" < "ab", empty < "a", "b" > "abc"]
}
