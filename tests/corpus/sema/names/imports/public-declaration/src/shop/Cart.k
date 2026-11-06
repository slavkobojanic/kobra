// An import binds the declaration's simple name in this file.
import catalog.Product

public struct Cart {
    let products: List<Product>
    //                 ^^^^^^^ resolves: decl catalog.Product

    func names() -> List<String> {
        return products.map { product => product.name }
        //     ^^^^^^^^ resolves: member shop.Cart.products
        //                               ^^^^^^^ resolves: local product 9:31
        //                                       ^^^^ resolves: member catalog.Product.name
    }
}
