// A private member is visible in the whole body of its declaration, with or without `self.`.
struct Account {
    private let pin: Int

    func matches(guess: Int) -> Bool {
        return guess == pin and self.pin > 0
        //              ^^^ resolves: member corpus.Account.pin
        //                           ^^^ resolves: member corpus.Account.pin
    }
}
