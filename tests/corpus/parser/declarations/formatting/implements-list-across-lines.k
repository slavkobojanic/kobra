// A comma at the end of a line continues the `implements` list.
service Payments implements
    PaymentProvider,
    Refunds,
    Webhooks
{
}
