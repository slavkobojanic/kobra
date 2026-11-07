import k.crypto.Uuid

/// Reference data seeded by a migration.
public entity Category {
    /// The primary key.
    @Id let id: Uuid

    /// The name.
    @Unique let name: String

    /// The display order.
    let rank: Int

    /// Shown on the front page.
    let featured: Bool = false
}
