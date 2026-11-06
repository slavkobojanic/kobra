import shop.catalog.Product

page ProductList at "/products" {
    @Param let page: Int = 1

    view {
        for product in products key product.id {
            match product.stock {
                case 0 { "Sold out" }
                case 1...5 { "Only {product.stock} left" }
                default { "{product.price.formatted(as: .currency)}" }
            }
        }
    }
}
