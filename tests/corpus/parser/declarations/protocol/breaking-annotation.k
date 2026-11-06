// `@Breaking("reason")` marks a deliberate breaking change.
@Breaking("total is now a Money")
public protocol CartProtocol {
    rpc total() -> Money
}
