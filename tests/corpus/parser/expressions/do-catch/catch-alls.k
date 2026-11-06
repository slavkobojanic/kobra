// `catch let error` and `catch _` are catch-alls
func f() {
    do { try a() } catch let error { log(error) }
    do { try b() } catch _ { }
}
