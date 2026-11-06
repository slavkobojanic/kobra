// Another module reads them through the imported type.
import catalog.Product

func describe(product: Product) -> String {
    let fallback = Product.sample
    //                     ^^^^^^ resolves: member catalog.Product.sample
    return "{product.label()} {fallback.name}"
    //               ^^^^^ resolves: member catalog.Product.label()
    //                                  ^^^^ resolves: member catalog.Product.name
}
