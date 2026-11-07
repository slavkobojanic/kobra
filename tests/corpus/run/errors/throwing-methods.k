// Methods on structs and classes throw typed errors like functions.
enum WalletError implements Error {
    case insufficient(missing: Int)
}

class Wallet {
    var balance: Int = 10

    func pay(amount: Int) throws WalletError {
        if amount > balance {
            throw WalletError.insufficient(missing: amount - balance)
        }
        balance -= amount
    }
}

func main() -> List<String> {
    let wallet = Wallet()
    var out: List<String> = []
    for amount in [4, 8, 6] {
        do {
            try wallet.pay(amount: amount)
            out.append("paid {amount}")
        } catch .insufficient(let missing) {
            out.append("missing {missing}")
        }
    }
    out.append("left {wallet.balance}")
    return out
}
