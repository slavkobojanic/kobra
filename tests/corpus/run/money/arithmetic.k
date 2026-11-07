// Money adds, subtracts and scales by an Int exactly, with no floating-point error.
func main() -> List<Money> {
    let dime = 0.10.usd
    let price = 24.00.usd
    return [dime + 0.20.usd, price * 3, price - 0.01.usd, 10.eur - 15.eur, 1500.jpy * 2]
}
