/// Splits a bill, with a tip, between the people at the table.
public page Home at "/" {
    @State var bill: String = ""
    @State var tip: Int = 15
    @State var people: Int = 2

    @Derived let billCents: Int = ((Double.parse(bill) ?? 0.0) * 100.0).rounded()
    @Derived let total: Money = Money(minorUnits: billCents * (100 + tip) / 100, currency: .usd)
    @Derived let share: Money = total.allocated(into: if people < 1 { 1 } else { people })[0]

    metadata {
        title("Tip")
    }

    view {
        main {
            heading(level: 1) { "Tip splitter" }

            textField(value: bind bill)
                .label("Bill")

            box {
                TipOption(percent: 10, isSelected: tip == 10)
                    .on(chosen: { percent => tip = percent })

                TipOption(percent: 15, isSelected: tip == 15)
                    .on(chosen: { percent => tip = percent })

                TipOption(percent: 20, isSelected: tip == 20)
                    .on(chosen: { percent => tip = percent })
            }
            .class(.tips)

            numberField(value: bind people)
                .label("People")

            paragraph { "Total with tip: {total.formatted(as: .currency)}" }

            paragraph { "Each person pays {share.formatted(as: .currency)}" }
                .class(.share)
        }
        .class(.page)
    }

    style {
        .page {
            maxWidth: 40rem;
            margin: 0 auto;
            padding: .lg;
            font: .body;
        }

        .tips {
            display: flex;
            gap: .sm;
        }

        .share { color: .primary; }
    }
}
