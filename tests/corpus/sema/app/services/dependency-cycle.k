// Two services that inject each other: each participant reports the field that closes the cycle.
service Orders {
    @Inject let billing: Billing
}

service Billing {
    @Inject let orders: Orders
}
