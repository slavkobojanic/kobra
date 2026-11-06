struct Account {
    private let secret: Int
    let name: String
}

func leak(account: Account) -> Int {
    return account.secret
}
