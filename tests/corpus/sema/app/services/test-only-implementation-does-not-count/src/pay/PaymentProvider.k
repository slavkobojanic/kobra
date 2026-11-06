// The interface.
public interface PaymentProvider {
    func charge(cents: Int) async
}
