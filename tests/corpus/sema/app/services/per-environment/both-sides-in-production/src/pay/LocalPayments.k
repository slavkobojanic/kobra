// Client-only: it injects `LocalStorage`.
import k.storage.LocalStorage

public service LocalPayments implements PaymentProvider {
    @Inject let storage: LocalStorage

    func charge(cents: Int) async {
    }
}
