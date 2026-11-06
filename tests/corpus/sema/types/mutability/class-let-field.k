// A class's `let` field can't be assigned outside its initializer, even through a `var` reference.
class Account {
    let id: Int
    var balance: Int = 0

    init(id: Int) {
        self.id = id
    }
}

func steal() -> Int {
    var account = Account(id: 1)
    account.balance = 5
    account.id = 2
    return account.id
}
