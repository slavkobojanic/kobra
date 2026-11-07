import k.crypto.Uuid

/// Queries and status changes for `Order`.
///
/// Status changes are conditional updates that name the status they expect
/// to move away from. Webhooks are delivered at least once, and an admin can
/// double-click; a transition that has already happened simply reports
/// `false` instead of applying twice.
public repository OrderRepository for Order {
    /// Returns the order with the given id, with its product loaded.
    ///
    /// - Parameter id: the order's id.
    /// - Returns: the order, or `nil` when there is none with this id.
    func byId(_ id: Uuid) async -> Order? {
        return await orders
            .including { order => order.product }
            .first { order => order.id == id }
    }

    /// Returns the most recently placed orders, newest first, with their
    /// products loaded.
    ///
    /// - Parameter limit: the maximum number of orders to return.
    func recent(limit: Int) async -> List<Order> {
        return await orders
            .including { order => order.product }
            .sortedByDescending { order => order.placedAt }
            .limit(limit)
    }

    /// Marks an order that is awaiting payment as paid.
    ///
    /// - Parameters:
    ///   - id: the order to mark.
    ///   - reference: the payment provider's id for the payment.
    ///   - paidAt: when the payment was confirmed.
    /// - Returns: `true` when this call marked the order paid, `false` when it
    ///   was not awaiting payment (for example, because a previous delivery of
    ///   the same webhook already marked it).
    func markPaid(id: Uuid, reference: String, paidAt: Date) async -> Bool {
        let updated = await orders
            .filter { order => order.id == id and order.status == .awaitingPayment }
            .update { order =>
                order.status = .paid
                order.paymentReference = reference
                order.paidAt = paidAt
            }

        return updated == 1
    }

    /// Moves an order from one status to another, if it is still in the
    /// expected one.
    ///
    /// - Parameters:
    ///   - id: the order to change.
    ///   - from: the status the order must currently have.
    ///   - to: the new status.
    /// - Returns: `true` when the order changed, `false` when it was not in
    ///   status `from`.
    func transition(id: Uuid, from: OrderStatus, to: OrderStatus) async -> Bool {
        let updated = await orders
            .filter { order => order.id == id and order.status == from }
            .update { order => order.status = to }

        return updated == 1
    }
}
