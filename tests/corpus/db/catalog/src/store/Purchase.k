import k.crypto.Uuid

/// A purchase of one product.
public entity Purchase {
    /// The primary key.
    @Id let id: Uuid

    /// What was bought: `product_id`.
    @Relation let product: Product

    /// Who bought it, in the renamed column `buyer_id`.
    let customer: CustomerId

    /// When.
    let placedAt: Date

    /// When it was paid, if it was.
    var paidAt: Date? = nil
}
