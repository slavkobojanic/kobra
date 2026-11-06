// Implicit members nest: the payload's type is the expected type of the inner `.name`.
import k.http.HttpStatus

enum ApiError implements Error {
    case upstream(HttpStatus)
}

let failure: ApiError = .upstream(.serviceUnavailable)
//                       ^^^^^^^^ resolves: member corpus.ApiError.upstream
//                                 ^^^^^^^^^^^^^^^^^^ resolves: member k.http.HttpStatus.serviceUnavailable
