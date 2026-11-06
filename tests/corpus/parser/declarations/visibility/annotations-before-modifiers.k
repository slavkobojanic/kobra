// Annotations come first, then visibility, then `static`.
service S {
    @Inject private let api: CartProtocol
    @State private static var hits: Int = 0
}
