// A checked literal is validated and normalized when you compile (20-standard-library.md, "Literals
// and parsing"), so the bundle holds its value as a constant: no parse, and no panic, at run time.
import k.crypto.Uuid
import k.text.EmailAddress

let sender = EmailAddress("  K Shop <orders@Shop.EXAMPLE> ")
let id = Uuid("0B6F2C1E-4D1A-4F8E-9A3B-2C5D6E7F8A9B")
let api = Url("https://api.stripe.com")

func main() -> String {
    return "{sender} {id} {api}"
}
