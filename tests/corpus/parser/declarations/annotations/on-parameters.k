// Parameters may carry annotations.
struct A {
    init(@Inject api: CartProtocol, @Default(1) count: Int) {}
}
