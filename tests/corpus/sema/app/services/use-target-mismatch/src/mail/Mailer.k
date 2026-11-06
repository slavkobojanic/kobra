// A service that doesn't implement `PaymentProvider`.
public service Mailer {
    func send(text: String) async {
    }
}
