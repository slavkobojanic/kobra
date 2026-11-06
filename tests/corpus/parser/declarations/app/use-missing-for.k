// `use X` without `for` reports K0100 at the gap after the type.
app A {
    use FakeApi
    use Real for Api
}
