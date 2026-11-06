struct OrderRequest {
    let productSlug: String
    let email: String
    var quantity: Int = 1
    var note: String? = nil
}

let complete = OrderRequest(productSlug: "mug", email: "a@example.com")
let explicit = OrderRequest(productSlug: "mug", email: "a@example.com", quantity: 2, note: "gift")
let partial = OrderRequest(productSlug: "mug")
