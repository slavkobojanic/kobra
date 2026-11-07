// Extensions add methods to built-in types; `self` is the receiver.
extend String {
    func shout() -> String {
        return self.uppercased() + "!"
    }

    func surrounded(by mark: String) -> String {
        return mark + self + mark
    }
}

func main() -> List<String> {
    let word = "hi"
    return [word.shout(), word.surrounded(by: "*"), "".shout()]
}
