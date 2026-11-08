// `HttpStatus(code:)` takes an integer literal from 100 to 599 (32-http-client.md), checked when you
// compile. Any other code is K2404.
import k.http.HttpStatus

let teapot = HttpStatus(code: 418)
let unavailable = HttpStatus(code: 503)

let tooLow = HttpStatus(code: 99)
let tooHigh = HttpStatus(code: 600)
