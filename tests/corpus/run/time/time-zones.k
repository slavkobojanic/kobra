// `TimeZone.parse` accepts IANA names (canonical case) and refuses others;
// a zone's identifier and name are its IANA name, and `utc` is "UTC"
// (35-time.md, 20-standard-library.md).
import k.time.TimeZone

func main() -> List<String?> {
    let berlin = TimeZone.parse("Europe/Berlin")
    let lower = TimeZone.parse("europe/berlin")
    let bogus = TimeZone.parse("Mars/Olympus_Mons")
    let empty = TimeZone.parse("")
    return [berlin?.identifier, berlin?.name, lower?.identifier, bogus?.identifier, empty?.identifier, TimeZone.utc.identifier]
}
