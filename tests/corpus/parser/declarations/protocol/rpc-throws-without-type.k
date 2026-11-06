// `throws` without a type in an rpc is K0129.
protocol P {
    rpc a() throws -> Int
    rpc b() -> Int
}
