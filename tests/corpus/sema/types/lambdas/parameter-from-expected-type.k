// An unannotated lambda parameter takes its type from the expected function type.
struct Product {
    let name: String
    let stock: Int
}

func names(products: List<Product>) -> List<String> {
    let inStock = products.filter { product => product.stock > 0 }
                                  //^^^^^^^ type: corpus.Product
    return inStock.map { product => product.name }
         //^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ type: List<String>
}
