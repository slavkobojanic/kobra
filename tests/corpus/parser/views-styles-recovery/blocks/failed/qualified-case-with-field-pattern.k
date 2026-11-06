// A qualified enum case pattern with a labeled field pattern.
page Product {
    failed(LoadError.http(status: let code)) {
        text { "Error {code}" }
    }
}
