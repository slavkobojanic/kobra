// The interface the app chooses an implementation for, per environment.
public interface PaymentProvider {
    func charge(cents: Int) async
}
