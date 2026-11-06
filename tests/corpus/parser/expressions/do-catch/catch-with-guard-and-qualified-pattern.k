// catch clauses take `where` guards and qualified case patterns
func f() {
    do {
        try sync()
    } catch .network(let e) where e.isTimeout {
        retry()
    } catch SyncError.unauthorized {
        login()
    }
}
