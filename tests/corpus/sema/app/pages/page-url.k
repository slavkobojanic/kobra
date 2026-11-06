// Every `Page` value has `url: Url`.
page Pricing {
    view {
        text { "{Page.pricing.url}" }
        //       ^^^^^^^^^^^^^^^^ type: Url
        //                    ^^^ resolves: member Page.url
    }
}
