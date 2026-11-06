// `use Config.sample` without `for` reports K0100 at the gap after the operand.
test Values {
    use Config.sample
    use Real for Api
}
