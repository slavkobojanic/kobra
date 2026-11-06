// The entity from docs/design/19-persistence.md.
entity User {
    @Id let id: UserId
    @Unique var email: String
    var name: String
    var lastSeen: Date
    @Relation var orders: List<Order>
}
