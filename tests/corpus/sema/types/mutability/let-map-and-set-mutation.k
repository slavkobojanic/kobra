// `let` maps and sets reject their mutating calls too; each call is its own K2011.
func build() -> Int {
    let ages: Map<String, Int> = [:]
    ages.set("Ana", 31)
    let tags: Set<String> = ["new"]
    tags.remove("new")
    return ages.count + tags.count
}
