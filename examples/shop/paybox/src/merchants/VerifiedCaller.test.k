test VerifiedCaller {
    @Inject let caller: VerifiedCaller

    test "paybox's own in-process code has no verified caller" {
        expect caller.name() == nil
    }
}
