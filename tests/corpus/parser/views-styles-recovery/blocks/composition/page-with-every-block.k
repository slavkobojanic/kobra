// A page with @Load fields and every block kind, in an unusual order (member order is free).
page Product {
    @Param let id: ProductId
    @Inject let products: ProductService
    @Load let product: Product = try await products.find(id)

    style {
        .title { fontSize: .xl; }
    }

    failed(let error) {
        ErrorBanner(message: error.message)
    }

    view {
        heading(level: 1) { product.name }
            .class(.title)
    }

    loading {
        ProductSkeleton()
    }

    metadata {
        title(product.name)
    }
}
