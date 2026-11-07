import k.crypto.Uuid
import k.rpc.RpcOnlyError
import k.time.Clock

/// The server side of `CatalogProtocol`.
public service CatalogApi implements CatalogProtocol {
    @Inject let products: ProductRepository
    @Inject let purchases: PurchaseRepository
    @Inject let kinds: CategoryRepository
    @Inject let clock: Clock

    rpc add(id: Uuid, slug: String, name: String, price: Money, weight: Decimal, stock: Int, note: String?) async throws RpcOnlyError -> ProductView {
        let product = Product(id: id, slug: slug, name: name, price: price, weight: weight, stock: stock, note: note)

        do {
            try await products.insert(product)
        } catch .slug {
            return view(product: product)
        }

        guard let stored = await products.bySlug(slug) else {
            return view(product: product)
        }

        return view(product: stored)
    }

    rpc publish(slug: String) async throws RpcOnlyError -> Int {
        return await products.publish(slug: slug)
    }

    rpc live() async throws RpcOnlyError -> List<String> {
        let found = await products.live()

        return found.map { product => product.slug }
    }

    rpc search(text: String) async throws RpcOnlyError -> List<String> {
        let found = await products.search(text: text)

        return found.map { product => product.slug }
    }

    rpc heavierThan(weight: Decimal) async throws RpcOnlyError -> List<String> {
        let found = await products.heavierThan(weight: weight)

        return found.map { product => product.slug }
    }

    rpc atMost(price: Money) async throws RpcOnlyError -> List<String> {
        let found = await products.atMost(price: price)

        return found.map { product => product.slug }
    }

    rpc noted(note: String?) async throws RpcOnlyError -> List<String> {
        let found = await products.noted(note: note)

        return found.map { product => product.slug }
    }

    rpc among(ids: List<Uuid>) async throws RpcOnlyError -> List<String> {
        let found = await products.among(ids: ids)

        return found.map { product => product.slug }
    }

    rpc buy(purchase: Uuid, product: Uuid, customer: Uuid, quantity: Int) async throws RpcOnlyError -> Bool {
        guard await products.reserve(id: product, quantity: quantity) else {
            return false
        }

        let all = await products.among(ids: [product])

        guard let bought = all.first else {
            return false
        }

        await purchases.insert(Purchase(id: purchase, product: bought, customer: CustomerId(value: customer), placedAt: clock.now))

        return true
    }

    rpc pay(purchase: Uuid) async throws RpcOnlyError -> Bool {
        return await purchases.markPaid(id: purchase, at: clock.now)
    }

    rpc history(customer: Uuid) async throws RpcOnlyError -> List<String> {
        let mine = await purchases.of(customer: CustomerId(value: customer))
        let unpaid = await purchases.unpaid()

        let names = mine.map { purchase => "{purchase.product.name} ({purchase.product.price})" }

        return names.sorted() + ["unpaid {unpaid}"]
    }

    rpc product(slug: String) async throws RpcOnlyError -> ProductView? {
        guard let found = await products.bySlug(slug) else {
            return nil
        }

        return view(product: found)
    }

    rpc active() async throws RpcOnlyError -> Int {
        return await products.active()
    }

    rpc categories() async throws RpcOnlyError -> List<String> {
        let ranked = await kinds.ranked()
        let featured = await kinds.featured()

        return ranked.map { c => c.name } + featured.map { c => "featured: {c.name}" }
    }
}
