// An explicit import shadows the prelude declaration of the same name.
import money.Money

func total(money: Money) -> Int {
//                ^^^^^ resolves: decl money.Money
    return money.cents
}
