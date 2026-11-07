/// Queries for `RefundRecord`.
public repository RefundRepository for RefundRecord {
    /// Returns the refund with the given id.
    ///
    /// - Parameter id: `Refund.id`.
    /// - Returns: the refund, or `nil` when there is none.
    func byId(_ id: String) async -> RefundRecord? {
        return await refundRecords.first { refund => refund.id == id }
    }

    /// Returns the refunds of one charge, oldest first.
    ///
    /// - Parameter chargeId: `Charge.id`.
    func forCharge(_ chargeId: String) async -> List<RefundRecord> {
        return await refundRecords
            .filter { refund => refund.chargeId == chargeId }
            .sortedBy { refund => refund.createdAt }
    }
}
