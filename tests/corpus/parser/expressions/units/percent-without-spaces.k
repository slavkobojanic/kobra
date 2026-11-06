// `10%3` lexes as `10%` then `3`: two expressions on one line (K0102), not a remainder (RA-4)
func f() {
    let a = 10%3
}
