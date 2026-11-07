import pay.Charge

/// Every charge taken since the server started.
@Scope(.server)
public service ChargeLog {
    /// The charges, oldest first.
    @State var charges: List<Charge> = []

    /// Records `charge`.
    func record(charge: Charge) {
        charges.append(charge)
    }
}
