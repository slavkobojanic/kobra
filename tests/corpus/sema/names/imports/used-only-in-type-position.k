// A use in a type annotation counts: no K1105.
import k.http.HttpStatus

let lastStatus: HttpStatus? = nil
//              ^^^^^^^^^^ resolves: decl k.http.HttpStatus
