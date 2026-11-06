// An rpc without a parameter list reports K0100 at the token where `(` was expected.
protocol P {
    rpc list -> Int
    rpc ok()
}
