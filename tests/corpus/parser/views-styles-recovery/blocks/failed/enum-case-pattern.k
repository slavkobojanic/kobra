// Any pattern is accepted by the parser in `failed(...)`; restricting it is semantic.
page Product {
    failed(.notFound) {
        NotFound()
    }
}
