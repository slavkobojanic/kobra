// The memberwise initializer works across modules, defaults included; a missing field is still K2008.
import catalog.Product

func sample() -> Product {
    let mug = Product(slug: "mug")
      //^^^ type: catalog.Product
    let stocked = Product(slug: "cup", stock: 3)
    return Product()
}
