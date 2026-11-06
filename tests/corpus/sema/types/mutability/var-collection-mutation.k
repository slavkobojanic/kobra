// Lists, maps and sets bound with `var` accept mutating calls.
func build() -> Int {
    var names = ["Ana"]
    names.append("Ben")
    names.insert("Cy", at: 0)
    var ages: Map<String, Int> = [:]
    ages.set("Ana", 31)
    var tags: Set<String> = ["new"]
    tags.insert("sale")
    return names.count + ages.count + tags.count
         //^^^^^^^^^^^ type: Int
}
