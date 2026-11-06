// A private function can't be called from outside its declaration's body: K1301 at the member name.
struct Ledger {
    let total: Int

    private func audit() -> Bool {
        return total > 0
    }
}

func check(ledger: Ledger) -> Bool {
    return ledger.audit()
}
