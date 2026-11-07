import k.crypto.Uuid
import k.server.PublicUrls
import k.text.EmailAddress
import pay.Payments
import payments.CheckoutRequest
import payments.PaymentError
import testing.FakePaybox

test PayboxCheckout {
    use FakePaybox for Payments
    use PayboxCallbackConfig.sample for PayboxCallbackConfig

    @Inject let checkout: PayboxCheckout
    @Inject let paybox: FakePaybox
    @Inject let urls: PublicUrls

    let request = CheckoutRequest(
        orderId: Uuid("33333333-3333-4333-8333-333333333333"),
        customerEmail: EmailAddress("ana@example.com"),
        description: "K Mug",
        unitPrice: 24.00.usd,
        quantity: 2,
        returnUrl: Url("https://shop.test/receipt/33333333-3333-4333-8333-333333333333"),
        cancelUrl: Url("https://shop.test/products/k-mug")
    )

    test "checkout registers the shop's callback with paybox and goes to the card form" {
        let url = try await checkout.startCheckout(request: request)

        expect url == urls.absolute(.payment(id: request.orderId))
        expect paybox.callbacks == [Url("http://shop.internal:8080")]
    }

    test "every checkout registers again, so paybox follows the shop when it moves" {
        try await checkout.startCheckout(request: request)
        try await checkout.startCheckout(request: request)

        expect paybox.callbacks.count == 2
    }
}

test PayboxCheckoutOffline {
    use PayboxCallbackConfig.sample for PayboxCallbackConfig

    @Inject let checkout: PayboxCheckout

    test "without a fake, paybox is a TestRemote, which is offline, so checkout is unavailable" {
        let request = CheckoutRequest(
            orderId: Uuid.random(),
            customerEmail: EmailAddress("ana@example.com"),
            description: "K Mug",
            unitPrice: 24.00.usd,
            quantity: 1,
            returnUrl: Url("https://shop.test/receipt"),
            cancelUrl: Url("https://shop.test/products/k-mug")
        )

        expect throws PaymentError.unavailable {
            try await checkout.startCheckout(request: request)
        }
    }
}
