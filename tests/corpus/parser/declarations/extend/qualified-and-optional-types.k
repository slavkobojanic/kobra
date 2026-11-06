// Any type may be extended, including qualified and optional ones.
extend shop.Money {}
extend String? {}
public extend Map<String, Int> {
    static let empty: Map<String, Int> = [:]
}
