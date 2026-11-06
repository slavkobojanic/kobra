// The import names the module path and then the declaration.
import shop.catalog.items.Product

func title(product: Product) -> String {
//                  ^^^^^^^ resolves: decl shop.catalog.items.Product
    return product.name
}
