// Inside an extension, the type's fields are referenced unqualified.
struct Wallet {
    let cents: Int
}

extend Wallet {
    var dollars: Int { cents / 100 }
                     //^^^^^ resolves: member corpus.Wallet.cents
}
