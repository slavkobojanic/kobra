// identifiers are ASCII (RA-21): `é` is one K0001 Error token, wrapped in an ErrorNode; the next statement parses
func f() {
    é
    let ok = 1
}
