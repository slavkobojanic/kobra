//! The examples in real, headless Chrome (`docs/compiler/testing.md`,
//! "Real-browser tests").
//!
//! `k_server/tests/browser.rs` and `dist_browser.rs` drive the examples in
//! a simulated DOM, which has no layout, no CSS cascade, no fonts, no
//! focus and no default actions, so a page can pass there and still be
//! broken for a person. These tests build and serve each example as its
//! README says (`k build` and a static host for the static-only apps,
//! `k run` / `k dev` for the others), open it in Chrome over the DevTools
//! protocol, and use it with real mouse and key events. They read what is
//! on screen: accessible roles and names from Chrome's accessibility tree,
//! visible text (not `textContent`), computed styles, the fonts actually
//! used, and `document.activeElement`. Every tab fails its test on a
//! console error, and every `https` request is answered by the test, so
//! nothing depends on the internet (weather's Open-Meteo is a fake, for
//! the browser through DevTools and for the server through
//! `K_DEV_FETCH_ORIGINS`).
//!
//! Every example runs its release client: the static ones are built with
//! `k build`, the served ones run with `k run --release` / `k dev
//! --release` (`K_CHROME_PROFILE=development` serves the development
//! client instead).
//!
//! Without Chrome the tests are skipped with a message; `K_REQUIRE_CHROME=1`
//! (CI) makes a missing Chrome fail them. `K_CHROME` names the executable.
//!
//! A test marked `#[ignore = "red: <defect>"]` reproduces a defect that is
//! not fixed yet (the ids are the diagnosis reports' and are listed in
//! `docs/compiler/testing.md`); the fix removes the `ignore`.
//!
//! ```text
//! cargo test -p k_cli --test it -- chrome::
//! cargo test -p k_cli --test it -- chrome:: --ignored        # the red ones
//! grep -n 'ignore = "red:' crates/k_cli/tests/*.rs     # what is still red
//! ```

// The helpers fail the calling test with a message.
#![allow(clippy::panic, clippy::unwrap_used)]

use crate::apps;
use crate::cdp;
use crate::page;

use std::time::{Duration, Instant};

use serde_json::{Value, json};

use apps::{Proc, Scratch, StaticSite, copy_example};
use cdp::{Fulfil, Paused};
use page::{El, Tab};

/// A tab in a fresh headless Chrome, or `return` from the test (skipped)
/// when there is no Chrome.
macro_rules! tab {
    ($dir:expr) => {
        match page::chrome(&$dir.0) {
            Some(t) => t,
            None => return,
        }
    };
}

/// Whether a computed `font-family` ends in a generic family, so text is
/// never drawn in the browser's default serif when the named font is
/// missing.
fn has_generic_family(family: &str) -> bool {
    let last = family
        .rsplit(',')
        .next()
        .unwrap_or("")
        .trim()
        .trim_matches('"');
    matches!(
        last,
        "sans-serif" | "serif" | "monospace" | "system-ui" | "ui-sans-serif" | "ui-serif"
    )
}

/// Asserts that the text of `el` is drawn in the theme's sans-serif stack,
/// not the browser's default Times.
fn assert_sans_serif(tab: &mut Tab, el: &El) {
    let family = tab.style(el, "font-family");
    let used = tab.used_fonts(el);
    assert!(
        has_generic_family(&family),
        "{}: font-family {family:?} has no generic fallback",
        el.what
    );
    assert!(
        !used.iter().any(|f| f.contains("Times")),
        "{}: drawn in {used:?} (font-family {family:?})",
        el.what
    );
}

/// Whether a computed colour is fully transparent.
fn transparent(color: &str) -> bool {
    color == "rgba(0, 0, 0, 0)" || color == "transparent" || color.is_empty()
}

/// The chart's horizontal letterboxing: how far the drawing starts inside
/// the `<svg>` box (0 when the chart fills its width).
fn chart_letterbox(tab: &mut Tab) -> f64 {
    tab.css("svg[role=img]");
    tab.eval(
        "(() => { const s = document.querySelector('svg[role=img]'); const r = s.getBoundingClientRect(); const m = s.getScreenCTM(); return m.e - r.left; })()",
    )
    .as_f64()
    .unwrap_or(0.0)
}

/// Asserts that the `<link rel=manifest>` exists and its file names `name`.
fn assert_manifest(tab: &mut Tab, name: &str) {
    let manifest = tab.eval(
        "(async () => { const l = document.querySelector('link[rel=manifest]'); if (!l) return null; const r = await fetch(l.href); return r.ok ? await r.json() : { status: r.status }; })()",
    );
    assert_eq!(
        manifest.get("name").and_then(Value::as_str),
        Some(name),
        "the web app manifest: {manifest}"
    );
}

// =================================================================== todos

/// The todos example's static build on a static host, and a tab on it.
fn todos(dir: &Scratch) -> (StaticSite, Tab) {
    let site = StaticSite::start("todos", &dir.0);
    let mut tab = match page::chrome(&dir.0) {
        Some(t) => t,
        None => panic!("unreachable: checked by the caller"),
    };
    tab.open(&format!("{}/", site.origin));
    tab.role("textbox", "New todo");
    (site, tab)
}

/// Whether Chrome is available (the skip message is printed once per
/// test by `page::chrome` otherwise).
fn have_chrome() -> bool {
    match cdp::chrome_binary() {
        Ok(_) => true,
        Err(why) => {
            assert!(
                std::env::var("K_REQUIRE_CHROME").as_deref() != Ok("1"),
                "K_REQUIRE_CHROME=1 but {why}"
            );
            eprintln!("skipped: {why}; this real-browser test did not run");
            false
        }
    }
}

/// Types `title` into the new-todo field and presses Enter.
fn add_todo(tab: &mut Tab, title: &str) {
    let field = tab.role("textbox", "New todo");
    tab.fill(&field, title);
    tab.press("Enter");
}

#[test]
fn todos_adds_completes_filters_and_clears_todos() {
    if !have_chrome() {
        return;
    }
    let dir = Scratch::new("todos-flow");
    let (_site, mut tab) = todos(&dir);
    tab.wait_text("Nothing to do yet. Add your first todo above.");
    add_todo(&mut tab, "Buy milk");
    tab.role("checkbox", "Buy milk");
    add_todo(&mut tab, "Walk the dog");
    let dog = tab.role("checkbox", "Walk the dog");
    tab.wait_text("2 items left");
    tab.click(&dog);
    tab.wait_text("1 item left");
    assert_eq!(tab.eval("document.title"), json!("Todos (1)"));
    // The filters: each shows only its todos, and says which one is on.
    let completed = tab.role("button", "Completed");
    tab.click(&completed);
    tab.wait("only the completed todo", |t| {
        (t.has_role("checkbox", "Walk the dog") && !t.has_role("checkbox", "Buy milk"))
            .then_some(())
    });
    let completed = tab.role("button", "Completed");
    assert_eq!(
        tab.on(
            &completed,
            "function() { return this.getAttribute('aria-pressed'); }",
            &[]
        ),
        json!("true")
    );
    let active = tab.role("button", "Active");
    tab.click(&active);
    tab.wait("only the active todo", |t| {
        (t.has_role("checkbox", "Buy milk") && !t.has_role("checkbox", "Walk the dog"))
            .then_some(())
    });
    let all = tab.role("button", "All");
    tab.click(&all);
    tab.wait("both todos", |t| {
        (t.has_role("checkbox", "Buy milk") && t.has_role("checkbox", "Walk the dog")).then_some(())
    });
    let clear = tab.role("button", "Clear completed");
    tab.click(&clear);
    tab.wait("the completed todo cleared", |t| {
        (!t.has_role("checkbox", "Walk the dog")).then_some(())
    });
    // Everything is kept in the browser: a reload shows the same list.
    tab.reload();
    tab.role("checkbox", "Buy milk");
    assert!(!tab.has_role("checkbox", "Walk the dog"));
}

/// The todo's title is on screen, next to its checkbox, as a `<label>`
/// (todos-A: a computed `.label` became `aria-label` only).
#[test]
fn todos_shows_each_todos_title_on_screen() {
    if !have_chrome() {
        return;
    }
    let dir = Scratch::new("todos-label");
    let (_site, mut tab) = todos(&dir);
    add_todo(&mut tab, "Buy milk");
    let checkbox = tab.role("checkbox", "Buy milk");
    let row = tab.closest(&checkbox, "li");
    let text = tab.text_of(&row);
    assert!(
        text.starts_with("Buy milk"),
        "the row shows its title before its buttons: {text:?}"
    );
    tab.by_text("Buy milk");
    let labels = tab.on(
        &checkbox,
        "function() { return [...(this.labels ?? [])].map((l) => l.innerText.trim()); }",
        &[],
    );
    assert!(
        labels.as_array().unwrap().contains(&json!("Buy milk")),
        "the visible title is the checkbox's <label>: {labels}"
    );
    // The rename field's label is on screen too.
    let edit = tab.role("button", "Edit Buy milk");
    tab.click(&edit);
    tab.role("textbox", "New title for Buy milk");
    tab.wait_text("New title for Buy milk");
}

/// Adding a todo happens in place: no reload, and the field is cleared
/// and keeps the focus for the next one (todos-B: `submit` handlers never
/// prevent the browser's form submission).
#[test]
fn todos_adds_in_place_and_keeps_the_focus_in_the_field() {
    if !have_chrome() {
        return;
    }
    let dir = Scratch::new("todos-submit");
    let (_site, mut tab) = todos(&dir);
    let token = tab.document_token();
    add_todo(&mut tab, "Buy milk");
    tab.role("checkbox", "Buy milk");
    tab.assert_same_document(&token, "adding a todo");
    let field = tab.role("textbox", "New todo");
    assert_eq!(
        tab.on(&field, "function() { return this.value; }", &[]),
        json!("")
    );
    assert!(
        tab.is_focused(&field),
        "the field keeps the focus, not {}",
        tab.focused()
    );
}

/// A blank title shows the alert and keeps what was typed, without a
/// reload (todos-B).
#[test]
fn todos_rejects_a_blank_title_with_an_alert_and_keeps_the_text() {
    if !have_chrome() {
        return;
    }
    let dir = Scratch::new("todos-blank");
    let (_site, mut tab) = todos(&dir);
    let token = tab.document_token();
    add_todo(&mut tab, "   ");
    tab.wait_text("Give the todo a title.");
    tab.assert_same_document(&token, "rejecting a blank title");
    let field = tab.role("textbox", "New todo");
    assert_eq!(
        tab.on(&field, "function() { return this.value; }", &[]),
        json!("   ")
    );
}

/// The dark scheme chosen with the switch is still applied after a reload
/// (todos-C: the remembered theme was read but never applied on load).
#[test]
fn todos_keeps_the_dark_theme_across_a_reload() {
    if !have_chrome() {
        return;
    }
    let dir = Scratch::new("todos-theme");
    let (_site, mut tab) = todos(&dir);
    let dark = tab.role("checkbox", "Dark mode");
    tab.click(&dark);
    tab.wait_js(
        "document.documentElement.classList.contains('k-theme-dark')",
        "the dark theme",
    );
    tab.reload();
    let dark = tab.role("checkbox", "Dark mode");
    assert_eq!(
        tab.on(&dark, "function() { return this.checked; }", &[]),
        json!(true)
    );
    assert_eq!(
        tab.eval("document.documentElement.className"),
        json!("k-theme-set k-theme-dark"),
        "the page is dark after the reload, as the switch says"
    );
}

/// Text is drawn in the theme's sans-serif stack, form controls inherit
/// it, and the page has no default margin (todos-D).
#[test]
fn todos_draws_text_in_a_sans_serif_stack_with_base_styles() {
    if !have_chrome() {
        return;
    }
    let dir = Scratch::new("todos-fonts");
    let (_site, mut tab) = todos(&dir);
    let heading = tab.role("heading", "Todos");
    assert_sans_serif(&mut tab, &heading);
    let add = tab.role("button", "Add");
    let page_font = tab.style(&heading, "font-family");
    assert_eq!(
        tab.style(&add, "font-family"),
        page_font,
        "buttons inherit the page's font"
    );
    let body = tab.css("body");
    assert_eq!(tab.style(&body, "margin-top"), "0px", "no default margin");
}

/// Toggling a todo keeps its row and the focus: the row updates in place
/// instead of being rebuilt (todos-E).
#[test]
fn todos_keeps_the_row_and_the_focus_when_a_todo_is_toggled() {
    if !have_chrome() {
        return;
    }
    let dir = Scratch::new("todos-row");
    let (_site, mut tab) = todos(&dir);
    add_todo(&mut tab, "Buy milk");
    let checkbox = tab.role("checkbox", "Buy milk");
    let row = tab.closest(&checkbox, "li");
    tab.click(&checkbox);
    tab.wait_text("0 items left");
    assert!(tab.is_connected(&row), "the row survives a mouse toggle");
    assert!(
        tab.is_focused(&checkbox),
        "the checkbox keeps the focus, not {}",
        tab.focused()
    );
    // And from the keyboard: Space toggles it back.
    tab.press("Space");
    tab.wait_text("1 item left");
    assert!(tab.is_connected(&row), "the row survives a keyboard toggle");
    assert!(
        tab.is_focused(&checkbox),
        "the checkbox keeps the focus, not {}",
        tab.focused()
    );
    // Edit moves the focus into the rename field.
    let edit = tab.role("button", "Edit Buy milk");
    tab.click(&edit);
    let field = tab.role("textbox", "New title for Buy milk");
    assert!(
        tab.is_focused(&field),
        "focus is in the rename field, not {}",
        tab.focused()
    );
}

/// The todos are stored under the app's namespace from `storage.lock`,
/// not a bare key another app on the origin could collide with (todos-F).
#[test]
fn todos_stores_its_todos_under_the_apps_namespace() {
    if !have_chrome() {
        return;
    }
    let dir = Scratch::new("todos-storage");
    let (_site, mut tab) = todos(&dir);
    add_todo(&mut tab, "Buy milk");
    tab.role("checkbox", "Buy milk");
    let keys = tab.eval("Object.keys(localStorage)");
    let keys: Vec<String> = serde_json::from_value(keys).unwrap();
    assert!(!keys.contains(&"todos".to_owned()), "a bare key: {keys:?}");
    assert!(
        keys.iter().any(|k| k.contains("todos") && k != "todos"),
        "a namespaced key: {keys:?}"
    );
}

/// The page's `description` and the app's `manifest` reach the document
/// (todos-G).
#[test]
fn todos_puts_its_description_and_manifest_in_the_head() {
    if !have_chrome() {
        return;
    }
    let dir = Scratch::new("todos-meta");
    let (_site, mut tab) = todos(&dir);
    assert_eq!(
        tab.eval("document.querySelector('meta[name=description]')?.content ?? null"),
        json!("A small todo list that keeps everything in your browser.")
    );
    assert_manifest(&mut tab, "Todos");
}

// ================================================================== kanban

/// The kanban example's static build, a tab on its home page, and a board
/// called "Launch plan" with `cards` in its first column.
fn kanban(dir: &Scratch, cards: &[&str]) -> (StaticSite, Tab) {
    let site = StaticSite::start("kanban", &dir.0);
    let mut tab = page::chrome(&dir.0).expect("checked by the caller");
    tab.open(&format!("{}/", site.origin));
    let field = tab.role("textbox", "New board");
    tab.fill(&field, "Launch plan");
    tab.press("Enter");
    tab.role("heading", "Launch plan");
    for card in cards {
        add_card(&mut tab, "To do", card);
    }
    (site, tab)
}

/// Adds a card through the column's "Add a card" button.
fn add_card(tab: &mut Tab, column: &str, title: &str) {
    let add = tab.role("button", &format!("Add a card to {column}"));
    tab.click(&add);
    let field = tab.role("textbox", &format!("New card in {column}"));
    tab.fill(&field, title);
    tab.press("Enter");
    tab.role("heading", title);
}

/// The visible text of the column `name`.
fn column_text(tab: &mut Tab, name: &str) -> String {
    let column = tab.role("region", name);
    tab.text_of(&column)
}

/// The draggable item of the card `title`.
fn card_item(tab: &mut Tab, title: &str) -> El {
    let heading = tab.role("heading", title);
    tab.closest(&heading, "li")
}

#[test]
fn kanban_creates_a_board_adds_cards_and_moves_them_with_keys_and_mouse() {
    if !have_chrome() {
        return;
    }
    let dir = Scratch::new("kanban-flow");
    let (_site, mut tab) = kanban(&dir, &["Write tests", "Ship it"]);
    assert!(column_text(&mut tab, "To do").contains("Write tests"));
    assert_eq!(tab.eval("document.title"), json!("Launch plan · Kanban"));
    // With the keyboard: pick up, move to the next column, drop.
    let card = card_item(&mut tab, "Write tests");
    tab.on(&card, "function() { this.focus(); }", &[]);
    tab.press("Space");
    tab.press("ArrowRight");
    tab.press("Space");
    tab.wait("the card in Doing", |t| {
        column_text(t, "Doing")
            .contains("Write tests")
            .then_some(())
    });
    let card = card_item(&mut tab, "Write tests");
    assert!(
        tab.is_focused(&card),
        "the moved card keeps the focus, not {}",
        tab.focused()
    );
    // With the mouse: drop "Ship it" onto "Write tests", taking its place.
    let ship = card_item(&mut tab, "Ship it");
    let target = card_item(&mut tab, "Write tests");
    let (x, y, w, h) = tab.rect(&target);
    tab.drag(&ship, (x + w / 2.0, y + h / 3.0));
    tab.wait("Ship it in Doing", |t| {
        column_text(t, "Doing").contains("Ship it").then_some(())
    });
    assert!(!column_text(&mut tab, "To do").contains("Ship it"));
    // Boards are kept in the browser.
    tab.reload();
    tab.role("heading", "Launch plan");
    assert!(column_text(&mut tab, "Doing").contains("Ship it"));
}

/// A card dragged with the mouse onto an empty column lands at its bottom
/// (kanban-RC1: only the list was a drop region, and an empty list has no
/// height).
#[test]
fn kanban_drops_a_card_dragged_onto_an_empty_column() {
    if !have_chrome() {
        return;
    }
    let dir = Scratch::new("kanban-empty-drop");
    let (_site, mut tab) = kanban(&dir, &["Ship it"]);
    let card = card_item(&mut tab, "Ship it");
    let done = tab.role("region", "Done");
    let (x, y, w, h) = tab.rect(&done);
    tab.drag(&card, (x + w / 2.0, y + h / 2.0));
    tab.wait("Ship it in Done", |t| {
        column_text(t, "Done").contains("Ship it").then_some(())
    });
}

/// Library tokens (`k.charts`, `k.drag`) and the theme's assignments to
/// them are custom properties on `:root` (kanban-RC2).
#[test]
fn kanban_puts_library_tokens_and_their_assignments_on_root() {
    if !have_chrome() {
        return;
    }
    let dir = Scratch::new("kanban-tokens");
    let (_site, mut tab) = kanban(&dir, &[]);
    assert_eq!(
        tab.root_style("--k-color-chart-grid").to_lowercase(),
        "#cbd5e1"
    );
    assert_eq!(
        tab.root_style("--k-color-drop-target").to_lowercase(),
        "#a5b4fc"
    );
    assert!(!tab.root_style("--k-font-size-chart-label").is_empty());
}

/// Text is drawn in a sans-serif stack, not Times (kanban-RC3).
#[test]
fn kanban_draws_text_in_a_sans_serif_stack() {
    if !have_chrome() {
        return;
    }
    let dir = Scratch::new("kanban-fonts");
    let (_site, mut tab) = kanban(&dir, &[]);
    let heading = tab.role("heading", "Launch plan");
    assert_sans_serif(&mut tab, &heading);
}

/// The theme picked with the picker survives a reload (kanban-RC4).
#[test]
fn kanban_keeps_the_chosen_theme_across_a_reload() {
    if !have_chrome() {
        return;
    }
    let dir = Scratch::new("kanban-theme");
    let (_site, mut tab) = kanban(&dir, &[]);
    let dark = tab.role("button", "Dark");
    tab.click(&dark);
    tab.wait_js(
        "document.documentElement.classList.contains('k-theme-dark')",
        "the dark theme",
    );
    tab.reload();
    tab.role("heading", "Launch plan");
    assert!(
        tab.eval("document.documentElement.classList.contains('k-theme-dark')") == json!(true),
        "dark after the reload: <html class={:?}>",
        tab.eval("document.documentElement.className")
    );
}

/// The document has base styles: no default body margin, card lists
/// without bullets, and the dark variant's `color-scheme` (kanban-RC5).
#[test]
fn kanban_has_base_styles_for_the_document_and_its_lists() {
    if !have_chrome() {
        return;
    }
    let dir = Scratch::new("kanban-base");
    let (_site, mut tab) = kanban(&dir, &["Ship it"]);
    let body = tab.css("body");
    assert_eq!(tab.style(&body, "margin-top"), "0px");
    let card = card_item(&mut tab, "Ship it");
    let list = tab.closest(&card, "ul");
    assert_eq!(tab.style(&list, "list-style-type"), "none");
    let dark = tab.role("button", "Dark");
    tab.click(&dark);
    tab.wait_js(
        "getComputedStyle(document.documentElement).colorScheme === 'dark'",
        "color-scheme: dark in the dark theme",
    );
}

/// Escape closes the card editor even while its field has the focus, as
/// the README says (reported as kanban-RC6; it does in real Chrome, so this
/// guards it).
#[test]
fn kanban_escape_closes_the_card_editor_while_typing() {
    if !have_chrome() {
        return;
    }
    let dir = Scratch::new("kanban-escape");
    let (_site, mut tab) = kanban(&dir, &["Ship it"]);
    let edit = tab.role("button", "Edit Ship it");
    tab.click(&edit);
    let field = tab.role("textbox", "Title of Ship it");
    tab.click(&field);
    assert!(
        tab.is_focused(&field),
        "typing in the title, not {}",
        tab.focused()
    );
    tab.type_text(" now");
    assert!(
        tab.has_role("textbox", "Title of Ship it"),
        "the editor is open"
    );
    tab.press("Escape");
    tab.wait("the editor closed", |t| {
        (!t.has_role("textbox", "Title of Ship it")).then_some(())
    });
}

/// Opening the new-card form puts the focus in its field, and the field's
/// label is on screen (kanban-RC6).
#[test]
fn kanban_focuses_the_new_card_field_and_shows_its_label() {
    if !have_chrome() {
        return;
    }
    let dir = Scratch::new("kanban-new-card");
    let (_site, mut tab) = kanban(&dir, &[]);
    let add = tab.role("button", "Add a card to To do");
    tab.click(&add);
    let field = tab.role("textbox", "New card in To do");
    assert!(
        tab.is_focused(&field),
        "focus is in the field, not {}",
        tab.focused()
    );
    tab.by_text("New card in To do");
}

/// The progress chart fills its width instead of being letterboxed in a
/// fixed `viewBox` (kanban-RC6, weather-B).
#[test]
fn kanban_chart_fills_its_width() {
    if !have_chrome() {
        return;
    }
    let dir = Scratch::new("kanban-chart");
    let (_site, mut tab) = kanban(&dir, &["Ship it"]);
    let offset = chart_letterbox(&mut tab);
    assert!(offset < 2.0, "the chart starts {offset}px inside its box");
}

/// The card editor's "Copy link" (`CopyButton`, 45-web-platform.md) puts
/// the card's absolute link on the real clipboard, says "Copied", and the
/// link opens the card's editor. The clipboard permission is granted over
/// DevTools, as a user's earlier "Allow" would be.
#[test]
fn kanban_copies_a_card_link_to_the_clipboard() {
    if !have_chrome() {
        return;
    }
    let dir = Scratch::new("kanban-copy-link");
    let (site, mut tab) = kanban(&dir, &["Ship it"]);
    tab.cdp.call(
        "Browser.grantPermissions",
        json!({
            "origin": site.origin,
            "permissions": ["clipboardReadWrite", "clipboardSanitizedWrite"],
        }),
    );
    let edit = tab.role("button", "Edit Ship it");
    tab.click(&edit);
    let copy = tab.role("button", "Copy link to Ship it");
    tab.click(&copy);
    tab.wait("the button says Copied", |t| {
        let copy = t.role("button", "Copy link to Ship it");
        (t.text_of(&copy) == "Copied").then_some(())
    });
    let copied = tab.eval("navigator.clipboard.readText()");
    let link = copied.as_str().unwrap_or_default().to_owned();
    let board = tab.path();
    assert!(
        link.starts_with(&format!("{}{board}?card=", site.origin)),
        "the clipboard holds the card's absolute link, not {copied}"
    );
    tab.open(&link);
    tab.role("textbox", "Title of Ship it");
}

// ================================================================= weather

/// Berlin as Open-Meteo answers `GET /v1/get?id=2950159`.
fn berlin() -> Value {
    json!({ "id": 2950159, "name": "Berlin", "latitude": 52.52, "longitude": 13.41, "country": "Germany", "admin1": "Land Berlin" })
}

/// Four hours of forecast from 2026-01-15T11:00Z, in Berlin's zone; the
/// wind blows from 250°.
fn forecast() -> Value {
    let start = 1_768_474_800_i64;
    json!({
        "timezone": "Europe/Berlin",
        "current": {
            "time": start, "temperature_2m": 3.4, "apparent_temperature": 0.6,
            "relative_humidity_2m": 81, "wind_speed_10m": 14.2, "wind_direction_10m": 250.0,
            "weather_code": 3, "is_day": 1
        },
        "hourly": {
            "time": [start, start + 3600, start + 7200, start + 10800],
            "temperature_2m": [3.4, 4.1, 4.6, 4.0],
            "apparent_temperature": [0.6, 1.2, 1.9, 1.1],
            "precipitation_probability": [10, 20, 35, 40]
        }
    })
}

fn air() -> Value {
    json!({ "current": { "european_aqi": 32, "pm2_5": 8.4, "pm10": 14.9 } })
}

fn search() -> Value {
    json!({ "results": [berlin(), { "id": 2661552, "name": "Bern", "latitude": 46.95, "longitude": 7.45, "country": "Switzerland" }] })
}

/// Open-Meteo's answer to `url`, for the browser.
fn open_meteo(p: &Paused) -> Option<Fulfil> {
    let host_path = p.url.strip_prefix("https://")?;
    let (host, rest) = host_path.split_once('/')?;
    if !host.ends_with("open-meteo.com") {
        return None;
    }
    let path = format!("/{}", rest.split('?').next().unwrap_or(""));
    let body = match path.as_str() {
        "/v1/search" => search(),
        "/v1/get" => berlin(),
        "/v1/forecast" => forecast(),
        "/v1/air-quality" => air(),
        _ => return None,
    };
    Some(Fulfil::json(&body))
}

/// The browser's requests to Open-Meteo's `path` (`/v1/forecast`, …).
fn meteo_requests(tab: &Tab, path: &str) -> usize {
    tab.requests()
        .iter()
        .filter(|r| r.url.contains("open-meteo.com") && r.url.contains(&format!("{path}?")))
        .count()
}

/// The weather example under `k run`, with Open-Meteo answered by a fake
/// on loopback for the server and through DevTools for the tab.
struct Weather {
    _fake: k_server::fakes::FakeHttp,
    _k: Proc,
    origin: String,
}

fn weather(dir: &Scratch, tab: &mut Tab) -> Weather {
    use k_server::fakes::{FakeHttp, FakeRoute};
    copy_example("weather", &dir.0.join("weather"));
    let fake = FakeHttp::serve(
        cdp::listener(),
        vec![
            FakeRoute::json("GET", "/v1/search", &search()),
            FakeRoute::json("GET", "/v1/get", &berlin()),
            FakeRoute::json("GET", "/v1/forecast", &forecast()),
            FakeRoute::json("GET", "/v1/air-quality", &air()),
        ],
    )
    .unwrap();
    let origins = [
        "https://geocoding-api.open-meteo.com",
        "https://api.open-meteo.com",
        "https://air-quality-api.open-meteo.com",
    ]
    .map(|o| format!("{o}={}", fake.origin()))
    .join(",");
    let port = cdp::free_port();
    let mut k = Proc::start(
        &["run", ".", "--port", &port.to_string()],
        &dir.0.join("weather"),
        &[("K_DEV_FETCH_ORIGINS", origins)],
        &[],
    );
    k.wait_port(port, Duration::from_secs(120));
    tab.intercept(open_meteo);
    Weather {
        _fake: fake,
        _k: k,
        origin: format!("http://localhost:{port}"),
    }
}

/// Opens Berlin's dashboard, rendered by the server.
fn open_berlin(tab: &mut Tab, w: &Weather) {
    tab.open(&format!("{}/city-weather/2950159", w.origin));
    tab.role("heading", "Berlin, Land Berlin, Germany");
}

#[test]
fn weather_searches_opens_a_city_and_switches_units() {
    let dir = Scratch::new("weather-flow");
    let mut tab = tab!(dir);
    let w = weather(&dir, &mut tab);
    tab.open(&format!("{}/", w.origin));
    let city = tab.role("textbox", "City");
    let token = tab.document_token();
    tab.fill(&city, "Ber");
    let berlin = tab.role("link", "Berlin, Land Berlin, Germany");
    tab.role("link", "Bern, Switzerland");
    tab.click(&berlin);
    tab.role("heading", "Berlin, Land Berlin, Germany");
    tab.assert_same_document(&token, "opening a search result");
    assert_eq!(tab.path(), "/city-weather/2950159");
    assert_eq!(tab.eval("document.title"), json!("Berlin weather"));
    tab.wait_text("3°C");
    let fahrenheit = tab.role("link", "°F");
    tab.click(&fahrenheit);
    tab.wait("the path with imperial units", |t| {
        t.path().contains("units=imperial").then_some(())
    });
    tab.wait_text("°F");
    tab.assert_same_document(&token, "switching units");
    // Back home through the layout's brand link.
    let home = tab.role("link", "Weather");
    tab.click(&home);
    tab.wait_text("Featured cities");
    drop(tab);
}

/// "Use my location" (`Geolocation`, 45-web-platform.md): the server
/// renders no button (it can't locate), the browser shows it after
/// hydration, and a click reads the position DevTools emulates, with the
/// permission granted as a user's earlier "Allow" would be, and opens the
/// weather there.
#[test]
fn weather_uses_my_location() {
    let dir = Scratch::new("weather-locate");
    let mut tab = tab!(dir);
    let w = weather(&dir, &mut tab);
    tab.cdp.call(
        "Browser.grantPermissions",
        json!({ "origin": w.origin, "permissions": ["geolocation"] }),
    );
    tab.cdp.call(
        "Emulation.setGeolocationOverride",
        json!({ "latitude": 48.8566, "longitude": 2.3522, "accuracy": 100 }),
    );
    tab.open(&format!("{}/", w.origin));
    let served = tab.eval("fetch('/').then((r) => r.text())");
    assert!(
        !served
            .as_str()
            .unwrap_or_default()
            .contains("Use my location"),
        "the server renders no button"
    );
    let locate = tab.role("button", "Use my location");
    let token = tab.document_token();
    tab.click(&locate);
    tab.role("heading", "Your location");
    tab.assert_same_document(&token, "opening the weather at the position");
    assert_eq!(tab.path(), "/local-weather/48.8566/2.3522");
    tab.wait_text("3°C");
    assert!(
        tab.requests()
            .iter()
            .any(|r| r.url.contains("/v1/forecast?") && r.url.contains("latitude=48.8566")),
        "the forecast for the position"
    );
    drop(tab);
}

/// The theme reaches the page: its font with a generic fallback, and the
/// page colours, in the dark scheme too (weather-A).
#[test]
fn weather_applies_the_theme_font_and_page_colours() {
    let dir = Scratch::new("weather-theme");
    let mut tab = tab!(dir);
    let w = weather(&dir, &mut tab);
    tab.prefer_dark();
    open_berlin(&mut tab, &w);
    let heading = tab.role("heading", "Berlin, Land Berlin, Germany");
    assert_sans_serif(&mut tab, &heading);
    let body = tab.css("body");
    let background = tab.style(&body, "background-color");
    assert!(
        !transparent(&background) && background != "rgb(255, 255, 255)",
        "the dark page background: {background}"
    );
    let color = tab.style(&heading, "color");
    assert_ne!(
        color, "rgb(0, 0, 0)",
        "the heading uses the theme's text colour"
    );
    drop(tab);
}

/// The chart has grid lines in the light theme: `k.charts`' tokens and
/// the theme's assignments to them are on `:root` (weather-B).
#[test]
fn weather_chart_has_grid_lines_in_the_light_theme() {
    let dir = Scratch::new("weather-grid");
    let mut tab = tab!(dir);
    let w = weather(&dir, &mut tab);
    open_berlin(&mut tab, &w);
    tab.css("svg[role=img] line");
    let strokes = tab.eval(
        "[...document.querySelectorAll('svg[role=img] line')].map((l) => getComputedStyle(l).stroke)",
    );
    assert!(
        strokes
            .as_array()
            .unwrap()
            .iter()
            .any(|s| s.as_str().is_some_and(|s| s != "none")),
        "grid line strokes: {strokes}"
    );
    assert_eq!(
        tab.root_style("--k-color-chart-grid").to_lowercase(),
        "#e2e8f0"
    );
    drop(tab);
}

/// `Icon` draws something: the condition icon and the wind arrow have a
/// size (weather-C: `Icon` lowered to an empty `<span>`).
#[test]
fn weather_draws_its_icons() {
    let dir = Scratch::new("weather-icons");
    let mut tab = tab!(dir);
    let w = weather(&dir, &mut tab);
    open_berlin(&mut tab, &w);
    let sizes = tab.eval(
        // By the icons' own attribute: release builds give classes short
        // names (codegen.md §17.5).
        "[...document.querySelectorAll('main [data-icon]')].map((e) => { const r = e.getBoundingClientRect(); return [r.width, r.height]; })",
    );
    let sizes = sizes.as_array().unwrap();
    assert_eq!(sizes.len(), 2, "two icons: {sizes:?}");
    for s in sizes {
        assert!(
            s[0].as_f64().unwrap_or(0.0) > 0.0 && s[1].as_f64().unwrap_or(0.0) > 0.0,
            "an icon has a size: {sizes:?}"
        );
    }
    drop(tab);
}

/// The wind arrow is rotated: a reactive `Angle` in a style is a CSS
/// angle with its unit (weather-D: emitted as a bare number).
#[test]
fn weather_rotates_the_wind_arrow() {
    let dir = Scratch::new("weather-wind");
    let mut tab = tab!(dir);
    let w = weather(&dir, &mut tab);
    open_berlin(&mut tab, &w);
    // The arrow's box: the element around its icon (release builds give
    // classes short names, codegen.md §17.5).
    let arrow = tab.css(":has(> [data-icon=\"arrowUp\"])");
    let rotate = tab.style(&arrow, "rotate");
    assert!(
        rotate.ends_with("deg") && rotate != "0deg",
        "the arrow's rotate: {rotate:?}"
    );
    drop(tab);
}

/// Switching units re-runs only the forecast: the place and the air
/// quality don't depend on units (weather-E1).
#[test]
fn weather_switching_units_fetches_only_the_forecast_again() {
    let dir = Scratch::new("weather-units");
    let mut tab = tab!(dir);
    let w = weather(&dir, &mut tab);
    open_berlin(&mut tab, &w);
    let (forecasts, airs, places) = (
        meteo_requests(&tab, "/v1/forecast"),
        meteo_requests(&tab, "/v1/air-quality"),
        meteo_requests(&tab, "/v1/get"),
    );
    let fahrenheit = tab.role("link", "°F");
    tab.click(&fahrenheit);
    tab.wait("the forecast in °F", |t| {
        (meteo_requests(t, "/v1/forecast") > forecasts).then_some(())
    });
    tab.wait_text("°F");
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(
        meteo_requests(&tab, "/v1/air-quality"),
        airs,
        "air quality again"
    );
    assert_eq!(meteo_requests(&tab, "/v1/get"), places, "the place again");
    drop(tab);
}

/// Shortens the dashboard's 10-minute refresh to `ms` and counts the
/// timers created for it in `window.__kRefreshTimers`.
fn fast_refresh(ms: u32) -> String {
    format!(
        "(() => {{ window.__kRefreshTimers = 0; const si = window.setInterval.bind(window); window.setInterval = (f, d, ...a) => {{ if (d === 600000) {{ window.__kRefreshTimers += 1; d = {ms}; }} return si(f, d, ...a); }}; }})();"
    )
}

/// The place a server-rendered dashboard was loaded with is kept by the
/// periodic refresh (`@Load(keepOnReload: true)`), so it isn't looked up
/// again (weather-E3).
#[test]
fn weather_refresh_after_server_rendering_uses_the_cached_place() {
    let dir = Scratch::new("weather-cache");
    let mut tab = tab!(dir);
    let w = weather(&dir, &mut tab);
    tab.before_each_document(&fast_refresh(1000));
    open_berlin(&mut tab, &w);
    tab.wait("a refresh", |t| {
        (meteo_requests(t, "/v1/forecast") >= 1).then_some(())
    });
    assert_eq!(
        meteo_requests(&tab, "/v1/get"),
        0,
        "the place was looked up again"
    );
    drop(tab);
}

/// The 10-minute refresh timer is created once, not again on every
/// refresh (weather-E2: the effect re-ran on each reload).
#[test]
fn weather_creates_the_refresh_timer_once() {
    let dir = Scratch::new("weather-timer");
    let mut tab = tab!(dir);
    let w = weather(&dir, &mut tab);
    tab.before_each_document(&fast_refresh(700));
    open_berlin(&mut tab, &w);
    tab.wait("two refreshes", |t| {
        (meteo_requests(t, "/v1/forecast") >= 2).then_some(())
    });
    assert_eq!(tab.eval("window.__kRefreshTimers"), json!(1));
    drop(tab);
}

/// The start page's recently viewed list is a client-rendered component
/// (47-client-rendered-components.md): the server's HTML has its empty
/// `loading` in its place, the browser shows the list from local storage
/// right after hydration, nothing the server rendered moves, and there are
/// no console errors (every tab fails its test on one).
#[test]
fn weather_shows_recent_places_after_hydration_without_moving_the_server_parts() {
    let dir = Scratch::new("weather-recent");
    let mut tab = tab!(dir);
    let w = weather(&dir, &mut tab);
    let home = format!("{}/", w.origin);
    // The server parts, with nothing stored: the list renders nothing.
    tab.open(&home);
    tab.wait_text("Featured cities");
    let server_parts = |tab: &mut Tab| {
        let parts = [
            tab.role("heading", "Weather"),
            tab.role("textbox", "City"),
            tab.role("heading", "Featured cities"),
            tab.role("link", "Berlin"),
            tab.role("link", "São Paulo"),
        ];
        parts.iter().map(|el| tab.rect(el)).collect::<Vec<_>>()
    };
    let before = server_parts(&mut tab);
    assert!(
        !tab.has_role("heading", "Recently viewed"),
        "nothing stored yet"
    );
    // Opening a city remembers it in local storage.
    open_berlin(&mut tab, &w);
    tab.open(&home);
    let html = tab.eval("fetch('/').then((r) => r.text())");
    let html = html.as_str().unwrap_or_default();
    assert!(
        html.contains("Featured cities"),
        "the server renders the page"
    );
    assert!(
        !html.contains("Recently viewed"),
        "the server renders the list's `loading`, not its view"
    );
    tab.role("heading", "Recently viewed");
    tab.role("link", "Berlin, Land Berlin, Germany");
    let after = server_parts(&mut tab);
    assert_eq!(before, after, "the server-rendered parts moved");
    drop(tab);
}

/// The app's manifest is linked and served (weather-F).
#[test]
fn weather_links_and_serves_its_manifest() {
    let dir = Scratch::new("weather-manifest");
    let mut tab = tab!(dir);
    let w = weather(&dir, &mut tab);
    tab.open(&format!("{}/", w.origin));
    assert_manifest(&mut tab, "Weather");
    drop(tab);
}

/// The temperature chart fills its card (weather-B, minor).
#[test]
fn weather_chart_fills_its_card() {
    let dir = Scratch::new("weather-chart");
    let mut tab = tab!(dir);
    let w = weather(&dir, &mut tab);
    open_berlin(&mut tab, &w);
    let offset = chart_letterbox(&mut tab);
    assert!(offset < 2.0, "the chart starts {offset}px inside its box");
    drop(tab);
}

// =================================================================== notes

/// The notes example under `k dev` in a scratch copy, with the README's
/// `.env` when `with_env`.
fn notes(dir: &std::path::Path, with_env: bool) -> (Proc, String) {
    copy_example("notes", dir);
    if with_env {
        std::fs::copy(dir.join(".env.example"), dir.join(".env")).unwrap();
    }
    let port = cdp::free_port();
    let mut k = Proc::start(
        &["dev", ".", "--port", &port.to_string()],
        dir,
        &[],
        &["DATABASE_URL", "DATABASE_PASSWORD"],
    );
    k.wait_port(port, Duration::from_secs(120));
    (k, format!("http://localhost:{port}"))
}

/// Signs up as `email` on the sign-up page and waits for the notes list.
fn sign_up(tab: &mut Tab, origin: &str, email: &str) {
    tab.open(&format!("{origin}/sign-up"));
    let field = tab.role("textbox", "Email");
    tab.fill(&field, email);
    let password = tab.css("input[type=password]");
    tab.fill(&password, "correct horse");
    let confirm = tab.role("textbox", "Confirm password");
    tab.fill(&confirm, "correct horse");
    let create = tab.role("button", "Create account");
    tab.click(&create);
    tab.role("link", "New note");
}

/// Writes a note titled `title` from the list page and waits for the list
/// to show it.
fn write_note(tab: &mut Tab, title: &str) {
    let new = tab.role("link", "New note");
    tab.click(&new);
    let field = tab.role("textbox", "Title");
    tab.fill(&field, title);
    let body = tab.role("textbox", "Note");
    tab.fill(&body, "Text");
    let create = tab.role("button", "Create note");
    tab.click(&create);
    tab.role("link", title);
}

#[test]
fn notes_signs_up_and_creates_edits_and_deletes_a_note() {
    let dir = Scratch::new("notes-flow");
    let mut tab = tab!(dir);
    let (_k, origin) = notes(&dir.0.join("notes"), true);
    sign_up(&mut tab, &origin, "ada@example.com");
    let new = tab.role("link", "New note");
    tab.click(&new);
    let title = tab.role("textbox", "Title");
    tab.fill(&title, "Groceries");
    let body = tab.role("textbox", "Note");
    tab.fill(&body, "Milk and eggs");
    let tags = tab.role("textbox", "Tags");
    tab.fill(&tags, "home");
    let create = tab.role("button", "Create note");
    tab.click(&create);
    let link = tab.role("link", "Groceries");
    assert_eq!(tab.path(), "/");
    // Edit it.
    tab.click(&link);
    let title = tab.role("textbox", "Title");
    tab.click(&title);
    tab.select_all();
    tab.type_text("Groceries for Sunday");
    let save = tab.role("button", "Save");
    tab.click(&save);
    tab.wait_text("Saved.");
    let home = tab.role("link", "K Notes");
    tab.click(&home);
    // Delete it.
    let link = tab.role("link", "Groceries for Sunday");
    tab.click(&link);
    let delete = tab.role("button", "Delete note");
    tab.click(&delete);
    tab.wait("the note gone", |t| {
        (t.path() == "/" && !t.has_role("link", "Groceries for Sunday")).then_some(())
    });
    // Sign out and back in.
    let out = tab.role("button", "Sign out");
    tab.click(&out);
    let sign_in = tab.role("link", "Sign in");
    tab.click(&sign_in);
    let email = tab.role("textbox", "Email");
    tab.fill(&email, "ada@example.com");
    let password = tab.css("input[type=password]");
    tab.fill(&password, "correct horse");
    let button = tab.role("button", "Sign in");
    tab.click(&button);
    tab.role("link", "New note");
    drop(tab);
}

/// `k dev` in a fresh checkout serves the app with no setup step: the
/// development database has a default (notes-1).
#[test]
fn notes_starts_with_one_command() {
    let dir = Scratch::new("notes-one-command");
    let mut tab = tab!(dir);
    let (_k, origin) = notes(&dir.0.join("notes"), false);
    sign_up(&mut tab, &origin, "ada@example.com");
    drop(tab);
}

/// The theme's font and colours reach the page (notes-2).
#[test]
fn notes_applies_the_theme_font_and_colours() {
    let dir = Scratch::new("notes-theme");
    let mut tab = tab!(dir);
    let (_k, origin) = notes(&dir.0.join("notes"), true);
    tab.open(&format!("{origin}/sign-in"));
    let button = tab.role("button", "Sign in");
    let label = tab.by_text("Email");
    assert_sans_serif(&mut tab, &label);
    let _ = button;
    let body = tab.css("body");
    let background = tab.style(&body, "background-color");
    assert!(
        !transparent(&background),
        "the page background: {background}"
    );
    drop(tab);
}

/// Two K apps on `localhost` keep their own sessions: signing in to one
/// doesn't sign the other out (notes-3: one fixed `k_session` cookie name,
/// and unknown cookies cleared).
#[test]
fn notes_two_apps_on_localhost_keep_their_own_sessions() {
    let dir = Scratch::new("notes-two-apps");
    let mut tab = tab!(dir);
    let (_a, first) = notes(&dir.0.join("first"), true);
    let (_b, second) = notes(&dir.0.join("second"), true);
    sign_up(&mut tab, &first, "ada@example.com");
    write_note(&mut tab, "Ada's note");
    sign_up(&mut tab, &second, "ben@example.com");
    tab.open(&format!("{first}/"));
    // Still Ada, in the first app: her note is listed.
    tab.role("link", "Ada's note");
    assert_eq!(tab.path(), "/", "still signed in to the first app");
    drop(tab);
}

/// The layout's offline banner (`Connectivity`, 45-web-platform.md) appears
/// when the browser loses its network, emulated over DevTools as a real
/// disconnection is (`navigator.onLine` turns `false` and `offline` fires),
/// and goes when the network is back.
#[test]
fn notes_shows_an_offline_banner_while_the_network_is_gone() {
    let dir = Scratch::new("notes-offline");
    let mut tab = tab!(dir);
    let (_k, origin) = notes(&dir.0.join("notes"), true);
    sign_up(&mut tab, &origin, "ada@example.com");
    let banner = "You're offline. K Notes can't save changes until the connection is back.";
    assert!(
        !tab.visible_text().contains(banner),
        "no banner while online"
    );
    let conditions = |offline: bool| {
        json!({
            "offline": offline,
            "latency": 0,
            "downloadThroughput": -1,
            "uploadThroughput": -1,
        })
    };
    tab.cdp.call("Network.enable", json!({}));
    tab.cdp
        .call("Network.emulateNetworkConditions", conditions(true));
    tab.wait_text(banner);
    tab.cdp
        .call("Network.emulateNetworkConditions", conditions(false));
    tab.wait_no_text(banner);
    // The list's stream drops while the network is gone; that is expected.
    tab.allow_errors();
    drop(tab);
}

/// Stopping the server with a page's live stream open finishes promptly
/// instead of waiting out the whole shutdown grace (notes-4).
#[test]
fn notes_stops_promptly_with_a_stream_open() {
    let dir = Scratch::new("notes-shutdown");
    let mut tab = tab!(dir);
    let (mut k, origin) = notes(&dir.0.join("notes"), true);
    sign_up(&mut tab, &origin, "ada@example.com");
    // The list's `watch()` stream is open now.
    std::thread::sleep(Duration::from_millis(500));
    let started = Instant::now();
    k.terminate();
    let exit = k.wait_exit(Duration::from_secs(5));
    assert!(
        exit.is_some(),
        "k dev still running {:?} after SIGTERM:\n{}",
        started.elapsed(),
        k.log()
    );
    // The page notices the server is gone; that is expected here.
    std::thread::sleep(Duration::from_millis(300));
    tab.allow_errors();
    drop(tab);
}

// ============================================================ stack, shop

#[test]
fn stack_pays_from_the_shop_page_through_paybox() {
    let dir = Scratch::new("stack-flow");
    let mut tab = tab!(dir);
    copy_example("stack", &dir.0.join("stack"));
    let port = cdp::free_ports(2);
    let mut k = Proc::start(
        &["dev", ".", "--port", &port.to_string()],
        &dir.0.join("stack"),
        &[],
        &[
            "PAYBOX_URL",
            "PAYBOX_TOKEN",
            "SHOP_URL",
            "SHOP_TOKEN",
            "K_SERVICE_TOKEN",
        ],
    );
    k.wait_port(port, Duration::from_secs(120));
    k.wait_port(port + 1, Duration::from_secs(60));
    tab.open(&format!("http://localhost:{}/", port + 1));
    tab.wait_text("Not paid yet");
    let pay = tab.role("button", "Pay");
    tab.click(&pay);
    tab.wait_text("Paid:");
    drop(tab);
}

/// The shop workspace under `k dev` at its root, as its README runs it:
/// no `.env`, no secrets to make, no catalog to load by hand. The
/// development database, the admin password, the demo catalog and the
/// mail catcher all come from `k dev` and the members' `[dev]` settings.
struct Shop {
    k: Proc,
    /// The shop member's directory.
    dir: std::path::PathBuf,
    origin: String,
}

fn shop(root: &std::path::Path) -> Shop {
    copy_example("shop", root);
    let paybox = cdp::free_ports(2);
    let shop_port = paybox + 1;
    let mut k = Proc::start(
        &["dev", ".", "--port", &paybox.to_string()],
        root,
        &[],
        &[
            "DATABASE_URL",
            "PAYBOX_URL",
            "PAYBOX_TOKEN",
            "PAYBOX_CALLBACK_URL",
            "K_SERVICE_TOKEN",
            "SMTP_HOST",
            "SMTP_PORT",
            "SHOP_ADMIN_PASSWORD_HASH",
            "SHOP_ADMIN_PASSWORD_HASH_FILE",
        ],
    );
    k.wait_port(paybox, Duration::from_secs(180));
    k.wait_port(shop_port, Duration::from_secs(60));
    Shop {
        k,
        dir: root.join("shop"),
        origin: format!("http://localhost:{shop_port}"),
    }
}

/// The development admin password `k dev` generated for the shop.
fn admin_password(s: &Shop) -> String {
    let file = s.dir.join(".k/secrets/SHOP_ADMIN_PASSWORD_HASH.password");
    std::fs::read_to_string(&file)
        .unwrap_or_else(|e| panic!("{}: {e}\n{}", file.display(), s.k.log()))
        .trim()
        .to_owned()
}

/// Buys one K Mug for `email` and pays with the test card that succeeds;
/// returns once the receipt is shown.
fn buy_a_mug(tab: &mut Tab, s: &Shop, email: &str) {
    tab.open(&format!("{}/", s.origin));
    let mug = tab.by_text("K Mug");
    tab.click(&mug);
    let field = tab.role("textbox", "Email for your receipt");
    tab.fill(&field, email);
    let buy = tab.role("button", "Buy now");
    tab.click(&buy);
    let card = tab.role("textbox", "Card number");
    tab.fill(&card, "4242 4242 4242 4242");
    let pay = tab.wait("the Pay button", |t| {
        t.roles("button")
            .into_iter()
            .find(|(name, _)| name.starts_with("Pay "))
            .map(|(_, el)| el)
    });
    tab.click(&pay);
    tab.wait("the receipt", |t| {
        let text = t.visible_text();
        (text.contains("Awaiting payment") || text.contains("Paid")).then_some(())
    });
}

/// Waits for an email to `to` in the development mail catcher's
/// `.k/mail/`, and returns it.
fn wait_mail(s: &Shop, to: &str) -> String {
    let mail = s.dir.join(".k/mail");
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        for e in std::fs::read_dir(&mail).into_iter().flatten().flatten() {
            let text = std::fs::read_to_string(e.path()).unwrap_or_default();
            if text.contains(to) {
                return text;
            }
        }
        assert!(
            Instant::now() < deadline,
            "no email to {to} in {}:\n{}",
            mail.display(),
            s.k.log()
        );
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// The README's purchase, on ports other than the default ones: paybox
/// calls the shop back at the address `k dev` gives it (shop-D3a), the
/// receipt turns paid, and the confirmation lands in the mail catcher.
#[test]
fn shop_sells_a_mug_through_paybox_and_emails_the_receipt() {
    let dir = Scratch::new("shop-flow");
    let mut tab = tab!(dir);
    let s = shop(&dir.0.join("shop"));
    buy_a_mug(&mut tab, &s, "ana@example.com");
    tab.wait_text("Paid");
    let mail = wait_mail(&s, "ana@example.com");
    assert!(mail.contains("K Mug"), "{mail}");
    drop(tab);
}

/// The shop's theme font reaches the page (shop-D4).
#[test]
fn shop_draws_text_in_the_themes_sans_serif_stack() {
    let dir = Scratch::new("shop-theme");
    let mut tab = tab!(dir);
    let s = shop(&dir.0.join("shop"));
    tab.open(&format!("{}/", s.origin));
    let mug = tab.by_text("K Mug");
    assert_sans_serif(&mut tab, &mug);
    drop(tab);
}

/// Signing out of the admin page leaves it for the server, which sends the
/// browser to the sign-in page, as `Admin.signOut` and the README say
/// (shop-D5: the example navigated home after the `await`, code that never
/// runs once the session has ended; 26-auth-and-sessions.md).
#[test]
fn shop_admin_signs_out_to_the_sign_in_page() {
    let dir = Scratch::new("shop-admin");
    let mut tab = tab!(dir);
    let s = shop(&dir.0.join("shop"));
    tab.open(&format!("{}/sign-in", s.origin));
    let email = tab.role("textbox", "Email");
    tab.fill(&email, "admin@shop.example");
    let password = tab.css("input[type=password]");
    let secret = admin_password(&s);
    tab.fill(&password, &secret);
    let button = tab.role("button", "Sign in");
    tab.click(&button);
    tab.role("button", "Sign out");
    assert_eq!(tab.path(), "/admin");
    let out = tab.role("button", "Sign out");
    tab.click(&out);
    tab.wait("the sign-in page", |t| {
        t.path().starts_with("/sign-in").then_some(())
    });
    tab.role("textbox", "Email");
    tab.role("button", "Sign in");
    drop(tab);
}

/// The cart stays in step across tabs (`TabMessaging`, 45-web-platform.md,
/// "Messages between tabs"): two tabs of one browser (two DevTools targets
/// in one browser context) on the shop; adding a mug in one shows it in the
/// other's header at once, while it is in the background and in the same
/// document, and emptying the cart in the second empties the first.
#[test]
fn shop_keeps_the_cart_in_step_across_tabs() {
    let dir = Scratch::new("shop-tabs");
    let mut first = tab!(dir);
    let s = shop(&dir.0.join("shop"));
    first.open(&format!("{}/", s.origin));
    first.wait_text("Cart: 0 items");
    let mut second = first.new_tab();
    second.open(&format!("{}/", s.origin));
    second.wait_text("Cart: 0 items");
    let token = second.document_token();
    first.bring_to_front();
    let mug = first.by_text("K Mug");
    first.click(&mug);
    let add = first.role("button", "Add to cart");
    first.click(&add);
    first.wait_text("Cart: 1 item");
    second.wait_text("Cart: 1 item");
    second.assert_same_document(&token, "the other tab's cart updating");
    second.bring_to_front();
    let empty = second.role("button", "Empty cart");
    second.click(&empty);
    second.wait_text("Cart: 0 items");
    first.wait_text("Cart: 0 items");
    drop(second);
    drop(first);
}
