// A function that changes a class instance it receives changes the caller's instance.
class Account {
    var balance: Int = 0

    func deposit(amount: Int) {
        balance += amount
    }
}

func payDay(account: Account) {
    account.deposit(amount: 100)
}

func main() -> Int {
    let account = Account()
    payDay(account: account)
    payDay(account: account)
    return account.balance
}
