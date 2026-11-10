test Home {
    test "the inferred picker keeps a fruit through the basket" {
        let screen = render(Home())

        screen.button(named: "Mango").click()

        expect screen.text("Picked Mango × 0").exists
        expect screen.text("Kept 1").exists
    }

    test "the picker with a written type argument keeps a quantity" {
        let screen = render(Home())

        screen.button(named: "Pear").click()
        screen.button(named: "3 pieces").click()

        expect screen.text("Picked Pear × 3").exists
        expect screen.text("Kept 2").exists
    }

    test "the injected ranker names the dearest fruit" {
        let screen = render(Home())

        expect screen.text("Dearest: Mango").exists
    }
}

test Basket {
    @Inject let basket: Basket

    test "a kept value decodes as the type asked for" {
        basket.keep(Fruit(name: "Kiwi", price: 2))

        expect basket.last<Fruit>() == Fruit(name: "Kiwi", price: 2)
        expect basket.last<Int>() == nil
    }
}

test HighestRanker {
    @Inject let ranker: Ranker

    test "the best item wins, the first on a tie" {
        expect ranker.best<String>(["a", "bb", "cc"], by: { s => s.count }) == "bb"
        expect ranker.best([3, 1, 2], by: { n => -n }) == 1
        expect ranker.best<Int>([], by: { n => n }) == nil
    }
}
