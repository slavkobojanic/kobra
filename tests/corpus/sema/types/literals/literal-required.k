// A checked literal's initializer takes a literal (20-standard-library.md, "Literals and parsing"):
// any other argument, an interpolated string included, is K2403, with a fix to `Type.parse` where
// the type has one. The fix is maybe-incorrect, because `parse` returns an optional.
import k.crypto.Uuid
import k.http.HttpStatus
import k.storage.StorageKey
import k.text.EmailAddress
import k.time.TimeOfDay

func make(text: String, code: Int) {
    let url = Url(text)
    let id = Uuid("{text}")
    let email = EmailAddress(text)
    let time = TimeOfDay(text)
    let status = HttpStatus(code: code)
    let key: StorageKey<String> = StorageKey(name: text)
    let parsed = Url.parse(text)
}
