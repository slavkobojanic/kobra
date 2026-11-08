// `TimeOfDay("…")` is checked when you compile (35-time.md): `HH:MM` on a 24-hour clock.
// Anything else is K2404.
import k.time.TimeOfDay

let nightly = TimeOfDay("03:00")
let late = TimeOfDay("23:59")

let pastMidnight = TimeOfDay("24:00")
let sixtyMinutes = TimeOfDay("12:60")
let oneDigit = TimeOfDay("3:00")
let word = TimeOfDay("noon")
