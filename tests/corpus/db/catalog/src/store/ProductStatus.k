/// Where a product is in its life.
public enum ProductStatus implements Codable {
    /// Not shown yet.
    case draft

    /// For sale.
    case live

    /// No longer sold.
    case retired
}
