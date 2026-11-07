// `?.` stops at the first nil in a chain, for fields and method calls alike.
struct Pet {
    let name: String?
}

struct Owner {
    let pet: Pet?
}

func main() -> List<Int?> {
    let full: Owner? = Owner(pet: Pet(name: "Rex"))
    let noName: Owner? = Owner(pet: Pet(name: nil))
    let noPet: Owner? = Owner(pet: nil)
    let nobody: Owner? = nil
    return [full?.pet?.name?.count, noName?.pet?.name?.count, noPet?.pet?.name?.count, nobody?.pet?.name?.count]
}
