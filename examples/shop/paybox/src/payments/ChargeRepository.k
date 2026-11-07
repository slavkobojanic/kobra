/// Queries and refund bookkeeping for `ChargeRecord`.
public repository ChargeRepository for ChargeRecord {
    /// Returns the charge with the given id.
    ///
    /// - Parameter id: `Charge.id`.
    /// - Returns: the charge, or `nil` when there is none.
    func byId(_ id: String) async -> ChargeRecord? {
        return await chargeRecords.first { charge => charge.id == id }
    }

    /// Returns the charge a merchant made for one of its references.
    ///
    /// - Parameter key: `"{merchant}/{reference}"`.
    /// - Returns: the charge, or `nil` when the reference wasn't charged.
    func byKey(_ key: String) async -> ChargeRecord? {
        return await chargeRecords.first { charge => charge.key == key }
    }

    /// Records a refund on a charge, if nothing else was refunded since
    /// `before` was read.
    ///
    /// The check and the write are a single conditional `UPDATE`, so two
    /// refunds racing for the same charge can't together refund more than
    /// its amount: the second sees a different `refunded` and changes
    /// nothing.
    ///
    /// - Parameters:
    ///   - id: the charge.
    ///   - before: the refunded amount the caller read.
    ///   - after: the new refunded amount.
    /// - Returns: `true` when this call recorded the refund.
    func recordRefund(id: String, before: Money, after: Money) async -> Bool {
        let updated = await chargeRecords
            .filter { charge => charge.id == id and charge.refunded == before }
            .update { charge => charge.refunded = after }

        return updated == 1
    }
}
