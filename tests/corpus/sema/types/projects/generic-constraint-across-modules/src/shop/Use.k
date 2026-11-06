// One conforming and one non-conforming type argument from another module.
import pricing.Catalog
import pricing.Priced

struct Mug implements Priced {
    let price: Int
}

struct Rock {
    let weight: Int
}

let mug = Catalog.cheapest(items: [Mug(price: 2)])
  //^^^ type: shop.Mug?
let rocks = [Rock(weight: 1)]
let rock = Catalog.cheapest(items: rocks)
