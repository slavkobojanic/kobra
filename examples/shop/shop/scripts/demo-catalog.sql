-- The demo catalog: a product in stock (k-mug), another one, and one that is
-- sold out (sticker-pack), for `k dev` and scripts/e2e.sh.
--
-- It is not a migration, because migrations run in every environment,
-- production and tests included, and are meant for data every environment
-- needs (docs/design/19-persistence.md#data-migrations). A real shop manages
-- its catalog from an admin screen.
--
-- The statement is plain SQL that SQLite and Postgres both accept. `k dev`
-- runs it once, when it creates the development database (`seed` in the
-- [dev] section of k.toml). With Compose, load it once the app has created
-- the tables:
--
--   docker compose exec -T db psql --username=shop --dbname=shop < scripts/demo-catalog.sql
--
-- `price` is a Money column, which the database stores as `price_amount` and
-- `price_currency` (docs/design/34-money.md).

INSERT INTO product (id, slug, name, summary, description, price_amount, price_currency, image_url, stock) VALUES
    (
        '0b6f2c1e-6a4f-4c39-9a57-2f1d3c4b5a61',
        'k-mug',
        'K Mug',
        'A 350 ml stoneware mug with the K logo, glazed by hand.',
        'Dishwasher safe, microwave safe, and heavy enough not to tip over when you push it aside to make room for a second monitor.',
        '24.00',
        'USD',
        'https://picsum.photos/seed/k-mug/800/800',
        25
    ),
    (
        '5d0a8e7b-1f2c-4e6d-8b3a-9c4e2f1a7d82',
        'field-notebook',
        'Field Notebook',
        'A pocket notebook with dot-grid pages for sketching views.',
        '48 pages of 100 g/m² paper, a lay-flat binding, and a cover that survives a year in a back pocket.',
        '12.00',
        'USD',
        'https://picsum.photos/seed/field-notebook/800/800',
        40
    ),
    (
        'a3e9c4d2-7b18-4f05-b6c1-3d2e8f9a0b13',
        'sticker-pack',
        'Sticker Pack',
        'Six vinyl stickers for your laptop lid.',
        'Weatherproof vinyl with a matte finish. Back in stock soon.',
        '6.00',
        'USD',
        'https://picsum.photos/seed/sticker-pack/800/800',
        0
    );
