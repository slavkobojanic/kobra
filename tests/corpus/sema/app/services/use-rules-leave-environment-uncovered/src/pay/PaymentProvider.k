// The interface the app chooses an implementation for.
public interface PaymentProvider {
    func charge(cents: Int) async
}
