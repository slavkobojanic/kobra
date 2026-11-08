// `Uuid("…")` is checked when you compile, as `Uuid.parse` checks text (33-crypto.md): 32
// hexadecimal digits in groups of 8-4-4-4-12, in either case. Anything else is K2404.
import k.crypto.Uuid

let lower = Uuid("0b6f2c1e-4d1a-4f8e-9a3b-2c5d6e7f8a9b")
let upper = Uuid("0B6F2C1E-4D1A-4F8E-9A3B-2C5D6E7F8A9B")

let short = Uuid("0b6f2c1e")
let notHex = Uuid("0b6f2c1e-4d1a-4f8e-9a3b-2c5d6e7f8a9g")
let braced = Uuid("{{0b6f2c1e-4d1a-4f8e-9a3b-2c5d6e7f8a9b}")
let undashed = Uuid("0b6f2c1e4d1a4f8e9a3b2c5d6e7f8a9b")
