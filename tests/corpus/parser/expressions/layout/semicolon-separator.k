// `;` is K0103 and recovers as a statement separator, with no K0102
func f() {
    let a = 1; let b = 2
    f();
}
