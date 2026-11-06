// `implements` lists several qualified and generic types.
public service StripePayments implements PaymentProvider, billing.Refunds, Handler<Webhook> {}
