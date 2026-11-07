import k.crypto.Uuid

/// Purchases.
public repository PurchaseRepository for Purchase {
    /// A customer's purchases with their products, oldest first (purchases in
    /// the same millisecond in any order).
    func of(customer: CustomerId) async -> List<Purchase> {
        return await purchases
            .filter { purchase => purchase.customer == customer }
            .including { purchase => purchase.product }
            .sortedBy { purchase => purchase.placedAt }
    }

    /// Marks an unpaid purchase paid.
    func markPaid(id: Uuid, at: Date) async -> Bool {
        let changed = await purchases
            .filter { purchase => purchase.id == id and purchase.paidAt == nil }
            .update { purchase => purchase.paidAt = at }

        return changed == 1
    }

    /// The number of unpaid purchases.
    func unpaid() async -> Int {
        return await purchases.filter { purchase => purchase.paidAt == nil }.count()
    }
}
