// The product page metadata from the metadata design: calls with strings, interpolation, implicit members and named args.
page Product {
    metadata {
        title("{product.name} · Acme Shop")
        description(product.summary)
        canonical(.product(id: id))
        socialPreview(image: product.imageUrl, title: product.name)
        structuredData(.product(name: product.name, price: product.price))
    }
}
