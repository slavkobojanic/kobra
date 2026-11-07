import pay.Charge

/// The charges paybox has told the shop about.
@Scope(.server)
public service PaidOrders {
    /// The charges, oldest first.
    @State var charges: List<Charge> = []

    /// Records `charge`.
    func record(charge: Charge) {
        charges.append(charge)
    }
}
