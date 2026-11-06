// Entities may implement interfaces and carry `@Breaking`.
@Breaking("dropped the legacy column")
public entity Order implements Auditable, Codable {
    @Id let id: OrderId
}
