// A public declaration of another module used without an import is K1202 with a fix that adds the import; `product.name` reports nothing more.
func title(product: Product) -> String {
    return product.name
}
