// In `u?.address.city`, `?.` unwraps `u` only: `address` is itself `Address?`, so
// `.city` on it needs `?.` exactly as in `v.address.city` (K2017). The chain must
// not swallow the optional of an intermediate member.
struct Address {
    let city: String
}

struct User {
    let address: Address?
}

func f(u: User?, v: User) -> Int {
    let h = u?.address.city
    let i = v.address.city
    let j = u?.address?.city
    return 0
}
