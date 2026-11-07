import k.crypto.Uuid
import k.db.Database
import k.observe.Log
import k.server.JobQueue
import k.time.Clock
import merchants.CallingMerchant
import merchants.MerchantAccount
import merchants.MerchantRepository
import pay.Charge
import pay.ChargeError
import pay.ChargeRequest
import pay.Merchant
import pay.Payments
import pay.Refund
import pay.RefundError
import pay.RefundRequest
import pay.RegistrationError

/// Paybox's implementation of `pay.Payments`.
///
/// Every change is recorded together with the event that tells the merchant
/// about it: the charge or refund row and its `EventDeliveryJob` are written
/// in one transaction, so a merchant hears about exactly the changes that
/// committed, at least once.
public service PaymentsApi implements Payments {
    @Inject let caller: CallingMerchant
    @Inject let merchants: MerchantRepository
    @Inject let charges: ChargeRepository
    @Inject let refunds: RefundRepository
    @Inject let database: Database
    @Inject let jobs: JobQueue
    @Inject let clock: Clock

    rpc registerMerchant(callbackUrl: Url) async throws RegistrationError -> Merchant {
        guard let name = caller.name() else {
            throw RegistrationError.unknownCaller
        }

        if await merchants.moveCallback(name: name, to: callbackUrl) {
            return Merchant(name: name, callbackUrl: callbackUrl)
        }

        let account = MerchantAccount(id: Uuid.random(), name: name, callbackUrl: callbackUrl, registeredAt: clock.now)

        do {
            try await merchants.insert(account)
            Log.info("Merchant {name} registered, with callbacks at {callbackUrl}")
        } catch .name {
            // Registered by a concurrent call a moment ago: take its row.
            await merchants.moveCallback(name: name, to: callbackUrl)
        }

        return account.toMerchant()
    }

    rpc charge(request: ChargeRequest) async throws ChargeError -> Charge {
        guard let name = caller.name(), let merchant = await merchants.byName(name) else {
            throw ChargeError.unknownMerchant
        }

        guard request.amount.minorUnits > 0 else {
            throw ChargeError.invalidAmount
        }

        let key = "{merchant.name}/{request.reference}"

        if let existing = await charges.byKey(key) {
            return existing.toCharge()
        }

        match TestCards.outcome(number: request.card.number) {
            case .invalid {
                throw ChargeError.invalidCard
            }

            case .declined(let reason) {
                Log.info("Declined {merchant.name}'s charge for {request.reference}: {reason}")

                throw ChargeError.declined(reason: reason)
            }

            case .approved {}
        }

        let record = ChargeRecord(
            id: "ch_{Uuid.random()}".replace("-", with: ""),
            key: key,
            merchant: merchant.name,
            reference: request.reference,
            amount: request.amount,
            last4: TestCards.lastFour(of: request.card.number),
            description: request.description,
            refunded: Money.zero(request.amount.currency),
            createdAt: clock.now
        )

        let created = await database.transaction {
            var inserted = true

            do {
                try await charges.insert(record)
            } catch .key {
                inserted = false
            }

            if inserted {
                await jobs.enqueue(EventDeliveryJob.kind, payload: EventDelivery(kind: .chargeSucceeded, merchant: merchant.name, subject: record.id))
            }

            inserted
        }

        // A concurrent request for the same reference may have charged it
        // first; then that charge is the answer, and this one never existed.
        if not created, let first = await charges.byKey(key) {
            return first.toCharge()
        }

        Log.info("Charged {request.amount} to {merchant.name}'s {request.reference} as {record.id}")

        return record.toCharge()
    }

    rpc refund(request: RefundRequest) async throws RefundError -> Refund {
        guard let name = caller.name(), let charge = await charges.byId(request.chargeId), charge.merchant == name else {
            throw RefundError.unknownCharge
        }

        guard request.amount.minorUnits > 0, request.amount.currency == charge.amount.currency else {
            throw RefundError.invalidAmount
        }

        let refundable = charge.amount - charge.refunded

        guard request.amount <= refundable else {
            throw RefundError.exceedsCharge(refundable: refundable)
        }

        let refund = RefundRecord(
            id: "re_{Uuid.random()}".replace("-", with: ""),
            chargeId: charge.id,
            merchant: charge.merchant,
            reference: charge.reference,
            amount: request.amount,
            createdAt: clock.now
        )

        let recorded = await database.transaction {
            let changed = await charges.recordRefund(id: charge.id, before: charge.refunded, after: charge.refunded + request.amount)

            if changed {
                await refunds.insert(refund)
                await jobs.enqueue(EventDeliveryJob.kind, payload: EventDelivery(kind: .chargeRefunded, merchant: charge.merchant, subject: refund.id))
            }

            changed
        }

        guard recorded else {
            // Another refund of this charge committed since it was read:
            // what is left to refund has changed.
            let left = await charges.byId(charge.id)?.refunded ?? charge.refunded

            throw RefundError.exceedsCharge(refundable: charge.amount - left)
        }

        Log.info("Refunded {request.amount} of {charge.id} as {refund.id}")

        return refund.toRefund()
    }
}
