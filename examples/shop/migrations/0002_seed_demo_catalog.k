// Written by hand with `k migrate new --empty "seed demo catalog"`.
// Format: docs/design/proposals/migrations.md (Status: proposal).

/// Adds the three demo products.
///
/// A real shop would manage its catalog from an admin screen. Seeding it in a
/// migration keeps the example self-contained and gives `scripts/e2e.sh` a
/// product in stock (`k-mug`) and one that is sold out (`sticker-pack`).
migration SeedDemoCatalog {
    func up(schema: Schema) {
        schema.insert(into: "product", rows: [
            [
                "id": .uuid("0b6f2c1e-6a4f-4c39-9a57-2f1d3c4b5a61"),
                "slug": .text("k-mug"),
                "name": .text("K Mug"),
                "summary": .text("A 350 ml stoneware mug with the K logo, glazed by hand."),
                "description": .text("Dishwasher safe, microwave safe, and heavy enough not to tip over when you push it aside to make room for a second monitor."),
                "price": .money(24.00.usd),
                "image_url": .text("https://picsum.photos/seed/k-mug/800/800"),
                "stock": .integer(25)
            ],
            [
                "id": .uuid("5d0a8e7b-1f2c-4e6d-8b3a-9c4e2f1a7d82"),
                "slug": .text("field-notebook"),
                "name": .text("Field Notebook"),
                "summary": .text("A pocket notebook with dot-grid pages for sketching views."),
                "description": .text("48 pages of 100 g/m² paper, a lay-flat binding, and a cover that survives a year in a back pocket."),
                "price": .money(12.00.usd),
                "image_url": .text("https://picsum.photos/seed/field-notebook/800/800"),
                "stock": .integer(40)
            ],
            [
                "id": .uuid("a3e9c4d2-7b18-4f05-b6c1-3d2e8f9a0b13"),
                "slug": .text("sticker-pack"),
                "name": .text("Sticker Pack"),
                "summary": .text("Six vinyl stickers for your laptop lid."),
                "description": .text("Weatherproof vinyl with a matte finish. Back in stock soon."),
                "price": .money(6.00.usd),
                "image_url": .text("https://picsum.photos/seed/sticker-pack/800/800"),
                "stock": .integer(0)
            ]
        ])
    }
}
