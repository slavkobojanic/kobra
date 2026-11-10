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

use apps::{Proc, Scratch, StaticSite, copy_example, copy_project, repo};
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

/// "Remind me" (`Notifications`, 45-web-platform.md): with the permission
/// granted as a user's earlier "Allow" would be, a todo's reminder is shown
/// by the static build's service worker, `/k-sw.js`, half an hour later.
/// Time passes quickly here: a script DevTools runs before the page's holds
/// each timer of a minute or more until the test calls `__kFastForward()`,
/// and from then on fires it at once and moves `Date.now` on by its delay,
/// so the page's once-a-minute check finds the reminder due. Holding them
/// first keeps "Reminder set" on screen until the test has seen it: with
/// time racing from the start the reminder could be shown and gone, and
/// the button back, between two looks of a test on a loaded machine.
///
/// The same script records each notification the service worker's
/// registration showed, once Chrome has shown it. The test reads that
/// record rather than `getNotifications()`, which on macOS asks the
/// system's notification center and, with several Chromes asking at once,
/// can take longer than the test waits (it made this test flaky in full
/// runs).
#[test]
fn todos_reminds_through_the_service_worker() {
    if !have_chrome() {
        return;
    }
    let dir = Scratch::new("todos-remind");
    let site = StaticSite::start("todos", &dir.0);
    let mut tab = tab!(dir);
    tab.cdp.call(
        "Browser.grantPermissions",
        json!({ "origin": site.origin, "permissions": ["notifications"] }),
    );
    tab.cdp.call(
        "Page.addScriptToEvaluateOnNewDocument",
        json!({ "source": "(() => { const now = Date.now; let skew = 0; Date.now = () => now() + skew; \
            const later = setTimeout, cancel = clearTimeout, held = new Map(); let fast = false, next = -1; \
            const fire = (f, d, a) => later(() => { skew += d; f(...a); }, 10); \
            globalThis.__kFastForward = () => { fast = true; for (const [f, d, a] of held.values()) fire(f, d, a); held.clear(); }; \
            globalThis.setTimeout = (f, d, ...a) => d < 60000 ? later(f, d, ...a) \
            : fast ? fire(f, d, a) : (held.set(next, [f, d, a]), next--); \
            globalThis.clearTimeout = (id) => (held.has(id) ? held.delete(id) : cancel(id)); \
            const shown = globalThis.__kShown = []; const show = ServiceWorkerRegistration.prototype.showNotification; \
            ServiceWorkerRegistration.prototype.showNotification = function (title, o) { \
            return show.call(this, title, o).then(() => { shown.push([title, o.body, o.data && o.data.url, this.active.scriptURL]); }); }; })();" }),
    );
    tab.open(&format!("{}/", site.origin));
    tab.role("textbox", "New todo");
    let worker =
        tab.eval("fetch('/k-sw.js').then((r) => [r.status, r.headers.get('content-type')])");
    assert_eq!(
        worker[0],
        json!(200),
        "the service worker is served at the root"
    );
    assert!(
        worker[1]
            .as_str()
            .unwrap_or_default()
            .contains("javascript"),
        "as JavaScript: {worker}"
    );
    add_todo(&mut tab, "Buy milk");
    let remind = tab.role("button", "Remind me about Buy milk");
    tab.click(&remind);
    tab.wait_text("Reminder set");
    tab.eval("__kFastForward()");
    let shown = tab.wait("the reminder shown by the service worker", |t| {
        let shown = t.eval("globalThis.__kShown");
        shown
            .as_array()
            .is_some_and(|a| !a.is_empty())
            .then_some(shown)
    });
    assert_eq!(
        shown,
        json!([[
            "Buy milk",
            "You asked to be reminded about this.",
            "/",
            format!("{}/k-sw.js", site.origin)
        ]])
    );
    let scope = tab.eval("navigator.serviceWorker.getRegistration('/').then((r) => r.scope)");
    assert_eq!(scope, json!(format!("{}/", site.origin)));
    tab.wait("the row offers a reminder again", |t| {
        t.has_role("button", "Remind me about Buy milk")
            .then_some(())
    });
    drop(tab);
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
        (t.has_role("checkbox", "Walk the dog") && t.has_no_role("checkbox", "Buy milk"))
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
        (t.has_role("checkbox", "Buy milk") && t.has_no_role("checkbox", "Walk the dog"))
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
        (t.has_no_role("checkbox", "Walk the dog")).then_some(())
    });
    // Everything is kept in the browser: a reload shows the same list.
    tab.reload();
    tab.role("checkbox", "Buy milk");
    assert!(tab.has_no_role("checkbox", "Walk the dog"));
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

/// Clicking "Add" puts the focus back in the new-todo field, so the next
/// todo can be typed at once (53-element-references.md, "Examples").
#[test]
fn todos_puts_the_focus_back_in_the_field_after_clicking_add() {
    if !have_chrome() {
        return;
    }
    let dir = Scratch::new("todos-click-add");
    let (_site, mut tab) = todos(&dir);
    let field = tab.role("textbox", "New todo");
    tab.fill(&field, "Buy milk");
    let add = tab.role("button", "Add");
    tab.click(&add);
    tab.role("checkbox", "Buy milk");
    tab.wait("the field has the focus", |t| {
        t.is_focused(&field).then_some(())
    });
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
    // The choice is this app's alone: its cookie is named after the port,
    // as the session cookie is (07-themes-and-units.md).
    assert_eq!(
        tab.eval(
            "document.cookie.split('; ').some(c => c === 'k_theme_' + location.port + '=dark')"
        ),
        json!(true),
        "the theme cookie is k_theme_<port>"
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

/// Escape closes the card editor, and, as the README says of every
/// shortcut, not while you type in one of its fields: there it would take
/// a key meant for the field (43-keyboard-shortcuts.md, "Rules").
///
/// This test used to say the opposite, that Escape closes the editor while
/// typing, and passed only by accident: its Escape carried a
/// `nativeVirtualKeyCode`, which on macOS hides the headless page, the
/// accessibility query for the field then never answered, and the wait
/// took the abandoned query for "the editor is gone". Under load the next
/// key event hung instead (the flake). See `Tab::press` and
/// `docs/compiler/testing.md`, "Flakes".
#[test]
fn kanban_escape_closes_the_card_editor_but_not_while_typing() {
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
    // The page has handled a key event when DevTools answers for it, and a
    // shortcut acts in its `keydown` handler, so this is already after it.
    tab.press("Escape");
    assert!(
        tab.has_role("textbox", "Title of Ship it") && tab.is_focused(&field),
        "Escape in the field leaves the editor open, still typing; the focus is on {}",
        tab.focused()
    );
    assert_eq!(
        tab.on(&field, "function() { return this.value; }", &[]),
        json!("Ship it now"),
        "the typing is kept"
    );
    // Out of the field (on a button of the editor), Escape closes it.
    let normal = tab.role("button", "Normal");
    tab.click(&normal);
    tab.press("Escape");
    tab.wait("the editor closed", |t| {
        (t.has_no_role("textbox", "Title of Ship it")).then_some(())
    });
}

/// Closing a card's editor, with Cancel or with Escape, puts the focus back
/// on the card's Edit button, and closing the new-card form on "Add a card"
/// (53-element-references.md, "Examples").
#[test]
fn kanban_returns_the_focus_to_the_card_after_closing_its_editor() {
    if !have_chrome() {
        return;
    }
    let dir = Scratch::new("kanban-focus-return");
    let (_site, mut tab) = kanban(&dir, &["Ship it"]);
    let edit = tab.role("button", "Edit Ship it");
    tab.click(&edit);
    let cancel = tab.role("button", "Cancel");
    tab.click(&cancel);
    tab.wait("the Edit button has the focus after Cancel", |t| {
        let edit = t.role("button", "Edit Ship it");
        t.is_focused(&edit).then_some(())
    });
    let edit = tab.role("button", "Edit Ship it");
    tab.click(&edit);
    let normal = tab.role("button", "Normal");
    tab.click(&normal);
    tab.press("Escape");
    tab.wait("the Edit button has the focus after Escape", |t| {
        let edit = t.role("button", "Edit Ship it");
        t.is_focused(&edit).then_some(())
    });
    let add = tab.role("button", "Add a card to To do");
    tab.click(&add);
    tab.role("textbox", "New card in To do");
    let cancel = tab.role("button", "Cancel");
    tab.click(&cancel);
    tab.wait("Add a card has the focus", |t| {
        let add = t.role("button", "Add a card to To do");
        t.is_focused(&add).then_some(())
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
    let (k, port) = Proc::serve(
        &["run", "."],
        1,
        &dir.0.join("weather"),
        &[("K_DEV_FETCH_ORIGINS", origins)],
        &[],
        Duration::from_secs(120),
    );
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
    tab.wait_handles(&city, "input");
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
    tab.wait_text("3°F");
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
    // The new values are on screen only once every load that re-ran has
    // answered, and loads that re-run start together, so by now an extra
    // request for the place or the air quality would have been made.
    tab.wait_text("3°F");
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
        tab.has_no_role("heading", "Recently viewed"),
        "nothing stored yet"
    );
    // Opening a city remembers it in local storage, once the page has
    // hydrated and its effect ran: leaving as soon as the server's heading
    // shows could leave before that.
    open_berlin(&mut tab, &w);
    tab.wait_js(
        "Object.keys(localStorage).some((k) => k.endsWith('recentPlaces') && localStorage.getItem(k).includes('2950159'))",
        "Berlin stored as recently viewed",
    );
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
    let (k, port) = Proc::serve(
        &["dev", "."],
        1,
        dir,
        &[],
        &["DATABASE_URL", "DATABASE_PASSWORD"],
        Duration::from_secs(120),
    );
    (k, format!("http://localhost:{port}"))
}

/// Signs up as `email` on the sign-up page and waits for the notes list.
fn sign_up(tab: &mut Tab, origin: &str, email: &str) {
    tab.open(&format!("{origin}/sign-up"));
    // Signing up while the passkey offer's challenge is still being
    // fetched is fine: its guest session has a cookie of its own
    // (docs/design/26-auth-and-sessions.md, "Guest sessions").
    let field = tab.role("textbox", "Email");
    tab.fill(&field, email);
    let password = tab.css("input[type=password]");
    tab.fill(&password, "correct horse");
    let confirm = tab.role("textbox", "Confirm password");
    tab.fill(&confirm, "correct horse");
    let create = tab.role("button", "Create account");
    tab.wait_handles_click(&create);
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
    // A slow page, as under a loaded machine: signing out used to race the
    // change stream's end, which the page answered by opening the stream
    // again under the dead session (a 401 in the console). The end now
    // carries the guest mirror, so the router leaves the page first.
    tab.cdp
        .call("Emulation.setCPUThrottlingRate", json!({ "rate": 4 }));
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
        (t.path() == "/" && t.has_no_role("link", "Groceries for Sunday")).then_some(())
    });
    // Sign out and back in. The guest mirror makes the router leave the
    // guarded list for a full load, which the server redirects to the
    // sign-in page. The header's "Sign in" link shows a moment before, as
    // soon as the session is gone, so following it, or typing before the
    // new page has loaded, races that load.
    let out = tab.role("button", "Sign out");
    let since = tab.cdp.events.len();
    tab.click(&out);
    tab.cdp
        .wait_event("Page.loadEventFired", since, Duration::from_secs(30))
        .expect("the sign-in page loads after signing out");
    assert_eq!(tab.path(), "/sign-in");
    let email = tab.role("textbox", "Email");
    tab.fill(&email, "ada@example.com");
    let password = tab.css("input[type=password]");
    tab.fill(&password, "correct horse");
    let button = tab.role("button", "Sign in");
    tab.wait_handles_click(&button);
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
    let offline_from = tab.errors().len();
    tab.cdp
        .call("Network.emulateNetworkConditions", conditions(true));
    tab.wait_text(banner);
    tab.cdp
        .call("Network.emulateNetworkConditions", conditions(false));
    tab.wait_no_text(banner);
    // The page's requests (the list's stream among them) fail with
    // ERR_INTERNET_DISCONNECTED while the network is gone; that error is
    // expected from the moment it went, even when Chrome reports it late,
    // and nothing else is.
    tab.allow_errors_containing(offline_from, "ERR_INTERNET_DISCONNECTED");
    drop(tab);
}

/// Stopping the server with a page's live stream open finishes promptly
/// instead of waiting out the whole shutdown grace (notes-4).
#[test]
fn notes_stops_promptly_with_a_stream_open() {
    let dir = Scratch::new("notes-shutdown");
    let mut tab = tab!(dir);
    let (mut k, origin) = notes(&dir.0.join("notes"), true);
    tab.cdp.call("Network.enable", json!({}));
    let since = tab.cdp.events.len();
    sign_up(&mut tab, &origin, "ada@example.com");
    // The list's `watch()` stream is open once its response head arrived.
    let opened = tab
        .cdp
        .wait_event_where("Network.responseReceived", since, page::WAIT, |e| {
            e["response"]["url"]
                .as_str()
                .is_some_and(|u| u.ends_with("NotesProtocol/Watch"))
        });
    assert!(opened.is_some(), "the notes list opens its watch() stream");
    let started = Instant::now();
    k.terminate();
    let exit = k.wait_exit(Duration::from_secs(5));
    assert!(
        exit.is_some(),
        "k dev still running {:?} after SIGTERM:\n{}",
        started.elapsed(),
        k.log()
    );
    // The page notices the server is gone, now and at every retry; that is
    // expected here.
    tab.allow_errors_from_now_on();
    drop(tab);
}

// ================================================================ channels

/// Records every channel socket the page opens (`__kSockets`) and the kind
/// of each frame it sends (`__kSent`), and refuses to open one while
/// `__kBlock` is set, as a network that is down would.
const SOCKET_SPY: &str = "(() => { const Real = WebSocket; globalThis.__kSockets = []; globalThis.__kSent = []; \
    globalThis.WebSocket = function (url, protocols) { if (globalThis.__kBlock) throw new Error('blocked'); \
    const s = new Real(url, protocols); const send = s.send.bind(s); \
    s.send = (t) => { __kSent.push(Object.keys(JSON.parse(t))[0]); send(t); }; __kSockets.push(s); return s; }; })();";

/// A channel in real Chrome against a real server
/// (`crates/k_server/tests/projects/channels` under `k dev`, 44-channels.md,
/// "The client"): the page opens its room channel on `/_k/channels`, gets
/// the snapshot, sends a message and sees the room's echo. Then the test
/// closes the socket from DevTools and keeps the network down for a
/// moment: the page shows it reconnecting, an announcement published to
/// the room meanwhile waits in the channel's buffer on the server, and
/// once a socket can open again the channel resumes with its token, which
/// delivers the announcement exactly once (a restart would have lost it:
/// it isn't in the snapshot) and repeats nothing.
#[test]
fn channels_resume_in_chrome_after_the_socket_drops() {
    let dir = Scratch::new("channels");
    let mut tab = tab!(dir);
    let root = dir.0.join("channels");
    copy_project(
        &repo().join("crates/k_server/tests/projects/channels"),
        &root,
    );
    let (k, port) = Proc::serve(
        &["dev", "."],
        1,
        &root,
        &[],
        &["DATABASE_URL"],
        Duration::from_secs(120),
    );
    let origin = format!("http://localhost:{port}");
    tab.before_each_document(SOCKET_SPY);
    tab.open(&format!("{origin}/"));
    let status = "document.querySelector('[role=status]').textContent === 'Open'";
    tab.wait_js(status, "the channel open");
    let field = tab.role("textbox", "Message");
    tab.fill(&field, "hello");
    tab.press("Enter");
    tab.wait_text("hello");
    assert_eq!(tab.eval("__kSockets.length"), json!(1));
    tab.eval("(globalThis.__kBlock = true, __kSockets[0].close(), 0)");
    tab.wait_text("Reconnecting");
    let announced = tab.eval(&format!(
        "fetch('{origin}/chans.chat.ChatProtocol/Announce', {{ method: 'POST', headers: {{ 'content-type': 'application/json', 'connect-protocol-version': '1' }}, body: JSON.stringify({{ room: 'lobby', text: 'while away' }}) }}).then((r) => r.status)"
    ));
    assert_eq!(announced, json!(200), "the announcement is published");
    tab.eval("(globalThis.__kBlock = false, 0)");
    tab.wait_js(status, "the channel open again");
    tab.wait_text("while away");
    let lines = tab.eval("[...document.querySelectorAll('li')].map((l) => l.textContent)");
    assert_eq!(
        lines,
        json!(["hello", "while away"]),
        "each line once, in order"
    );
    let sent = tab.eval("__kSent");
    let sent: Vec<&str> = sent
        .as_array()
        .unwrap()
        .iter()
        .filter_map(Value::as_str)
        .collect();
    assert!(
        sent.contains(&"resume") && sent.iter().filter(|k| **k == "open").count() == 1,
        "resumed with its token, never opened again: {sent:?}"
    );
    assert_eq!(tab.eval("__kSockets.length"), json!(2));
    drop(tab);
    drop(k);
}

// ============================================================ stack, shop

#[test]
fn stack_pays_from_the_shop_page_through_paybox() {
    let dir = Scratch::new("stack-flow");
    let mut tab = tab!(dir);
    copy_example("stack", &dir.0.join("stack"));
    let (_k, port) = Proc::serve(
        &["dev", "."],
        2,
        &dir.0.join("stack"),
        &[],
        &[
            "PAYBOX_URL",
            "PAYBOX_TOKEN",
            "SHOP_URL",
            "SHOP_TOKEN",
            "K_SERVICE_TOKEN",
        ],
        Duration::from_secs(120),
    );
    tab.open(&format!("http://localhost:{}/", port + 1));
    tab.wait_text("Not paid yet");
    let pay = tab.role("button", "Pay");
    tab.wait_handles_click(&pay);
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
    let (k, paybox) = Proc::serve(
        &["dev", "."],
        2,
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
        Duration::from_secs(180),
    );
    let shop_port = paybox + 1;
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
    tab.wait_handles_click(&buy);
    tab.click(&buy);
    let card = tab.role("textbox", "Card number");
    tab.fill(&card, "4242 4242 4242 4242");
    let pay = tab.wait("the Pay button", |t| {
        t.roles("button")
            .into_iter()
            .find(|(name, _)| name.starts_with("Pay "))
            .map(|(_, el)| el)
    });
    tab.wait_handles_click(&pay);
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
    tab.wait_handles_click(&button);
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
    first.wait_handles_click(&add);
    first.click(&add);
    first.wait_text("Cart: 1 item");
    second.wait_text("Cart: 1 item");
    second.assert_same_document(&token, "the other tab's cart updating");
    second.bring_to_front();
    let empty = second.role("button", "Empty cart");
    second.wait_handles_click(&empty);
    second.click(&empty);
    second.wait_text("Cart: 0 items");
    first.wait_text("Cart: 0 items");
    drop(second);
    drop(first);
}

/// How many random inputs each side of the crypto differential seals and
/// signs for the other.
const CRYPTO_ROUNDS: usize = 400;

/// Runs `body` on the server's `ops` backend: a fresh isolate with the
/// runtime's `crypto.js` as `c`, `codecs`, `hex` and `bin`, returning the
/// JSON text `body` assigns to `out`.
fn crypto_on_ops(body: &str) -> Value {
    let mut e = k_server::Engine::new(k_server::EngineOptions {
        timeout: Some(Duration::from_secs(60)),
        ..Default::default()
    })
    .expect("an isolate");
    let code = format!(
        "import * as c from \"k:/runtime/crypto.js\";\n\
         import {{ cryptoOps }} from \"k:/runtime/crypto-ops.js\";\n\
         import {{ codecs }} from \"k:runtime\";\n\
         c.useOps(cryptoOps(Deno.core.ops));\n\
         const hex = (b) => Array.from(b, (x) => x.toString(16).padStart(2, \"0\")).join(\"\");\n\
         const bin = (h) => Uint8Array.from(h.match(/../g) ?? [], (x) => parseInt(x, 16));\n\
         let out = null;\n\
         {body}\n\
         Deno.core.ops.op_k_set_result(JSON.stringify(out));\n"
    );
    let text = e.run_main(&code).expect("runs").expect("a result");
    serde_json::from_str(&text).expect("JSON")
}

/// `k.crypto` in real Chrome (33-crypto.md, "Testing"): the `subtle`
/// backend, Web Crypto, passes the same published vectors as the server's
/// `ops` backend (`crypto-vectors.js` over `crypto-vectors.json`: NIST
/// AES-GCM, RFC 5869, 8032, 7748 and 9180, and a Wycheproof subset; and the
/// key-format, RSA and JOSE vectors: RFC 7515 and 8037, Wycheproof RSA,
/// JWS, JWK sets and X25519 JWK and PEM), and
/// each side opens and verifies what the other sealed and signed on
/// random inputs: `Sealed` under one AES key, Ed25519 and ECDSA P-256
/// signatures, and `Envelope`s to each other's X25519 key. The runtime's
/// files are served as they are written; the other side is an isolate of
/// the server's runtime in this process.
#[test]
fn crypto_subtle_matches_the_vectors_and_the_ops_backend() {
    let dir = Scratch::new("crypto");
    let site = dir.0.join("site");
    let js = repo().join("crates/k_runtime/js");
    for sub in ["runtime", "tests"] {
        std::fs::create_dir_all(site.join(sub)).unwrap();
    }
    for entry in std::fs::read_dir(js.join("runtime")).unwrap().flatten() {
        std::fs::copy(entry.path(), site.join("runtime").join(entry.file_name())).unwrap();
    }
    for name in ["crypto-vectors.js", "crypto-vectors.json"] {
        std::fs::copy(js.join("tests").join(name), site.join("tests").join(name)).unwrap();
    }
    std::fs::write(
        site.join("index.html"),
        "<!doctype html><title>crypto</title>",
    )
    .unwrap();
    let host = crate::static_host::StaticHost::start(&site);
    let mut tab = tab!(dir);
    tab.open(&format!("http://localhost:{}/index.html", host.addr.port()));
    let prelude = "const c = await import('/runtime/crypto.js'); \
                   const { codecs } = await import('/runtime/std.js'); \
                   const hex = (b) => Array.from(b, (x) => x.toString(16).padStart(2, '0')).join(''); \
                   const bin = (h) => Uint8Array.from(h.match(/../g) ?? [], (x) => parseInt(x, 16));";

    // The vectors, on Web Crypto.
    let failures = tab.eval(&format!(
        "(async () => {{ {prelude} \
           if (c.backend() !== c.subtle) return ['the browser picked ' + c.backend().name]; \
           const {{ checkVectors, checkJoseVectors }} = await import('/tests/crypto-vectors.js'); \
           const v = await (await fetch('/tests/crypto-vectors.json')).json(); \
           return [...(await checkVectors(c, c.subtle, v)), ...(await checkJoseVectors(c, c.subtle, v))]; }})()"
    ));
    let failures: Vec<&str> = failures
        .as_array()
        .expect("a list")
        .iter()
        .filter_map(Value::as_str)
        .collect();
    assert!(
        failures.is_empty(),
        "{} vector(s) failed on Web Crypto:\n{}",
        failures.len(),
        failures.join("\n")
    );

    // The differential: one AES key both sides import, and each side's
    // own signing and agreement keys.
    let aes = "8f0e5d2c7b1a4f3e9d8c7b6a5f4e3d2c1b0a99887766554433221100ffeeddcc";
    let browser = tab.eval(&format!(
        "(async () => {{ {prelude} \
           globalThis.$keys = {{ aes: await c.subtle.fromRaw('AesGcmKey', bin('{aes}'), false), \
             ed: await c.Ed25519PrivateKey$generate(null), ec: await c.EcdsaP256PrivateKey$generate(null), \
             x: await c.X25519PrivateKey$generate(null) }}; \
           return {{ x: c.X25519PrivateKey$publicKey($keys.x) }}; }})()"
    ));
    let browser_x = browser["x"]
        .as_str()
        .expect("the browser's X25519 key")
        .to_owned();
    // The server's half: seals, signs and seals to the browser.
    let produce = |rand: &str| {
        format!(
            "const items = []; \
             for (let i = 0; i < {CRYPTO_ROUNDS}; i++) {{ \
               const msg = {rand}(i % 257); const ctx = {rand}(i % 7); \
               items.push({{ msg: hex(msg), ctx: hex(ctx), \
                 sealed: await c.AesGcmKey$seal(keys.aes, null, codecs.bytes, msg, ctx), \
                 ed: await c.Ed25519PrivateKey$sign(keys.ed, null, msg), \
                 ec: await c.EcdsaP256PrivateKey$sign(keys.ec, null, msg), \
                 env: await c.X25519PublicKey$seal(peerX, null, codecs.bytes, msg, ctx) }}); }}"
        )
    };
    // The checks of the other side's items: the number that failed.
    let consume = "let failed = []; \
         for (const [i, t] of theirs.items.entries()) { \
           const msg = bin(t.msg); const ctx = bin(t.ctx); \
           try { \
             if (hex(await c.AesGcmKey$open(keys.aes, null, codecs.bytes, t.sealed, ctx)) !== t.msg) failed.push(i + ' sealed'); \
             if (!(await c.Ed25519PublicKey$verify(theirs.ed, null, t.ed, msg))) failed.push(i + ' ed25519'); \
             if (!(await c.EcdsaP256PublicKey$verify(theirs.ec, null, t.ec, msg))) failed.push(i + ' ecdsa'); \
             if (hex(await c.X25519PrivateKey$open(keys.x, null, codecs.bytes, t.env, ctx)) !== t.msg) failed.push(i + ' envelope'); \
           } catch (e) { failed.push(i + ' threw ' + JSON.stringify(e?.message ?? e)); } }";
    let server = crypto_on_ops(&format!(
        "const keys = {{ aes: c.ops.fromRaw('AesGcmKey', bin('{aes}'), false), ed: c.TestKeys.ed25519(1), \
           ec: c.TestKeys.ecdsaP256(1), x: c.TestKeys.x25519(1) }}; \
         const peerX = {browser_x:?}; \
         {} \
         out = {{ ed: c.Ed25519PrivateKey$publicKey(keys.ed), ec: c.EcdsaP256PrivateKey$publicKey(keys.ec), \
           x: c.X25519PrivateKey$publicKey(keys.x), items }};",
        produce("c.random")
    ));
    // The browser opens and verifies the server's, and makes its own.
    let browser = tab.eval(&format!(
        "(async () => {{ {prelude} \
           const keys = $keys; const theirs = {server}; const peerX = theirs.x; \
           const random = (n) => crypto.getRandomValues(new Uint8Array(n)); \
           {consume} \
           {} \
           return {{ failed, ed: c.Ed25519PrivateKey$publicKey(keys.ed), ec: c.EcdsaP256PrivateKey$publicKey(keys.ec), items }}; }})()",
        produce("random")
    ));
    let failed = browser["failed"].as_array().expect("a list");
    assert!(
        failed.is_empty(),
        "the browser couldn't open or verify {} of the server's values: {failed:?}",
        failed.len()
    );
    // The server opens and verifies the browser's.
    let theirs =
        serde_json::json!({ "ed": browser["ed"], "ec": browser["ec"], "items": browser["items"] });
    let server = crypto_on_ops(&format!(
        "const keys = {{ aes: c.ops.fromRaw('AesGcmKey', bin('{aes}'), false), x: c.TestKeys.x25519(1) }}; \
         const theirs = {theirs}; \
         {consume} \
         out = failed;"
    ));
    let failed = server.as_array().expect("a list");
    assert!(
        failed.is_empty(),
        "the server couldn't open or verify {} of the browser's values: {failed:?}",
        failed.len()
    );
    drop(tab);
    drop(host);
}

/// `BrowserKeyStore` in real Chrome (33-crypto.md, "Keeping keys in the
/// browser"): a key put in the store survives a reload as a
/// non-extractable `CryptoKey` in IndexedDB, even one generated
/// exportable, and opens what it sealed before the reload; `delete`
/// forgets it, and `requestPersistence` answers one of the two cases.
#[test]
fn crypto_key_store_keeps_a_non_extractable_key_across_a_reload() {
    let dir = Scratch::new("key-store");
    let site = dir.0.join("site");
    let js = repo().join("crates/k_runtime/js");
    std::fs::create_dir_all(site.join("runtime")).unwrap();
    for entry in std::fs::read_dir(js.join("runtime")).unwrap().flatten() {
        std::fs::copy(entry.path(), site.join("runtime").join(entry.file_name())).unwrap();
    }
    std::fs::write(
        site.join("index.html"),
        "<!doctype html><title>keys</title>",
    )
    .unwrap();
    let host = crate::static_host::StaticHost::start(&site);
    let mut tab = tab!(dir);
    let url = format!("http://localhost:{}/index.html", host.addr.port());
    tab.open(&url);
    let prelude = "const c = await import('/runtime/crypto.js'); \
                   const { codecs } = await import('/runtime/std.js'); \
                   const store = c.BrowserKeyStore('keys-test'); \
                   const name = c.KeyName$init('vault');";
    let before = tab.eval(&format!(
        "(async () => {{ {prelude} \
           const key = await c.AesGcmKey$generateExportable(null); \
           await store.put(null, key, name); \
           return {{ sealed: await c.AesGcmKey$seal(key, null, codecs.string, 'kept', new Uint8Array(0)), \
             extractable: key.handle.extractable }}; }})()"
    ));
    assert_eq!(before["extractable"], json!(true), "{before}");
    let sealed = before["sealed"]
        .as_str()
        .expect("a sealed value")
        .to_owned();
    tab.reload();
    let after = tab.eval(&format!(
        "(async () => {{ {prelude} \
           const kept = await store.get(null, name); \
           const opened = await c.AesGcmKey$open(kept, null, codecs.string, {sealed:?}, new Uint8Array(0)); \
           const raw = await new Promise((resolve) => {{ \
             const r = indexedDB.open('k-keys:keys-test'); \
             r.onsuccess = () => {{ const g = r.result.transaction('keys').objectStore('keys').get('vault'); \
               g.onsuccess = () => resolve(g.result); }}; }}); \
           const persistence = await store.requestPersistence(null); \
           await store.delete(null, name); \
           return {{ opened, type: kept.type, printed: String(kept), \
             extractable: raw.handle.extractable, stored: Object.keys(raw).sort(), \
             gone: (await store.get(null, name)) === null, persistence: persistence.$ }}; }})()"
    ));
    assert_eq!(after["opened"], json!("kept"), "{after}");
    assert_eq!(after["type"], json!("AesGcmKey"));
    assert_eq!(after["printed"], json!("AesGcmKey(•••)"));
    assert_eq!(
        after["extractable"],
        json!(false),
        "the stored key is non-extractable"
    );
    assert_eq!(after["stored"], json!(["handle", "pub", "type"]));
    assert_eq!(after["gone"], json!(true), "{after}");
    assert!(
        after["persistence"] == json!(0) || after["persistence"] == json!(1),
        "{after}"
    );
    drop(tab);
    drop(host);
}

/// The notes vault in real Chrome (33-crypto.md, "Example: private
/// notes"): a note's private text is sealed in the browser, so the server
/// stores only `k1.` text; the key this browser kept in IndexedDB opens it
/// again after a reload; once locked, a wrong passphrase is refused and the
/// right one unlocks it again.
#[test]
fn notes_keeps_a_private_note_sealed_in_the_vault() {
    let dir = Scratch::new("notes-vault");
    let mut tab = tab!(dir);
    let (_k, origin) = notes(&dir.0.join("notes"), true);
    sign_up(&mut tab, &origin, "ada@example.com");
    write_note(&mut tab, "Diary");
    let link = tab.role("link", "Diary");
    tab.click(&link);
    let private = tab.role("link", "Private text");
    tab.click(&private);
    let set_up = tab.role("button", "Set up the vault");
    let passphrase = tab.css("input[type=password]");
    tab.fill(&passphrase, "a vault passphrase");
    tab.wait_handles_click(&set_up);
    tab.click(&set_up);
    let text = tab.role("textbox", "Private text");
    tab.fill(&text, "dear diary");
    let save = tab.role("button", "Save");
    tab.click(&save);
    tab.wait_text("Saved, sealed in this browser.");
    // What the server holds is sealed text.
    let id = tab
        .path()
        .rsplit('/')
        .next()
        .expect("the note's id")
        .to_owned();
    let stored = tab.eval(&format!(
        "fetch('/notes.vault.VaultProtocol/PrivateText', {{ method: 'POST', \
           headers: {{ 'Content-Type': 'application/json', 'Connect-Protocol-Version': '1' }}, \
           body: JSON.stringify({{ id: {id:?} }}) }}).then((r) => r.text())"
    ));
    let stored = stored.as_str().expect("the rpc's answer");
    assert!(
        stored.contains("k1.") && !stored.contains("dear diary"),
        "{stored}"
    );
    // The key this browser kept opens it after a reload.
    tab.reload();
    tab.wait("the private text opened again", |t| {
        let v = t.eval("document.querySelector('textarea')?.value ?? null");
        (v == json!("dear diary")).then_some(())
    });
    // Locked, a wrong passphrase is refused, and the right one opens it.
    let lock = tab.role("button", "Lock the vault");
    tab.click(&lock);
    let unlock = tab.role("button", "Unlock");
    let passphrase = tab.css("input[type=password]");
    tab.fill(&passphrase, "not the passphrase");
    tab.click(&unlock);
    tab.wait_text("That passphrase doesn't open the vault.");
    let passphrase = tab.css("input[type=password]");
    tab.click(&passphrase);
    tab.select_all();
    tab.type_text("a vault passphrase");
    let unlock = tab.role("button", "Unlock");
    tab.click(&unlock);
    tab.wait("the private text after unlocking", |t| {
        let v = t.eval("document.querySelector('textarea')?.value ?? null");
        (v == json!("dear diary")).then_some(())
    });
    drop(tab);
}

// ------------------------------------------------------------ media

/// Chrome's fake camera and microphone (46-media-and-rtc.md, "Testing the
/// stdlib against real browsers"), and `insecure.test` resolving to
/// loopback: an `http` origin that isn't `localhost`, so not a secure
/// context.
const FAKE_DEVICES: &[&str] = &[
    "--use-fake-device-for-media-stream",
    "--autoplay-policy=no-user-gesture-required",
    "--host-resolver-rules=MAP insecure.test 127.0.0.1",
];

/// [`FAKE_DEVICES`] with every prompt answered "Allow".
const FAKE_MEDIA: &[&str] = &[
    "--use-fake-device-for-media-stream",
    "--use-fake-ui-for-media-stream",
    "--autoplay-policy=no-user-gesture-required",
    "--host-resolver-rules=MAP insecure.test 127.0.0.1",
];

/// The capturing fixture (`fixtures/capturing`) under `k dev`.
struct Lab {
    _k: Proc,
    origin: String,
}

/// The capturing fixture under `k dev`, and a Chrome with `flags`.
fn capture_lab(dir: &Scratch, flags: &[&str]) -> Option<(Tab, Lab)> {
    let tab = page::chrome_with(&dir.0, flags)?;
    let fixture =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/capturing");
    copy_project(&fixture, &dir.0.join("capturing"));
    let (k, port) = Proc::serve(
        &["dev", "."],
        1,
        &dir.0.join("capturing"),
        &[],
        &[],
        Duration::from_secs(120),
    );
    Some((
        tab,
        Lab {
            _k: k,
            origin: format!("http://localhost:{port}"),
        },
    ))
}

/// Whether this Chrome opens its fake devices here. On macOS, Chrome asks
/// the system for the microphone even when it is fake (the fake camera
/// opens without asking; checked with Chrome's own flags and a fake audio
/// file, 2026-11), and a process that was never granted it (a terminal or an agent's sandbox
/// without the permission) waits forever: the tests that need a live
/// stream are then skipped with a message, unless
/// `K_REQUIRE_MEDIA=1` (CI) makes them fail.
fn can_capture(tab: &mut Tab) -> bool {
    let answer = tab.eval(
        "Promise.race([navigator.mediaDevices.getUserMedia({ audio: true }).then((s) => (s.getTracks().forEach((t) => t.stop()), 'ok'), (e) => e.name), new Promise((r) => setTimeout(() => r('timeout'), 5000))])",
    );
    if answer == json!("ok") {
        return true;
    }
    assert!(
        std::env::var("K_REQUIRE_MEDIA").as_deref() != Ok("1"),
        "K_REQUIRE_MEDIA=1 but Chrome's fake devices answered {answer}"
    );
    eprintln!(
        "skipped: Chrome's fake devices answered {answer} (on macOS, the process that runs the tests needs camera and microphone access); this part did not run"
    );
    false
}

/// Clicks the lab's button `name` and waits for its status line to start
/// with `expected`.
fn lab_click(tab: &mut Tab, name: &str, expected: &str) -> String {
    let button = tab.role("button", name);
    tab.wait_handles_click(&button);
    tab.click(&button);
    tab.wait(
        &format!("the status after {name:?} to start with {expected:?}"),
        |t| {
            let status = t.eval("document.querySelector('[role=status]')?.innerText ?? ''");
            let text = status.as_str().unwrap_or_default().to_owned();
            text.starts_with(expected).then_some(text)
        },
    )
}

/// The `CaptureError` rows Chrome can be made to produce (46, "Errors"),
/// through the production `BrowserMediaDevices`: before any answer the
/// permission is `prompt`, so a capture is `.notAsked` without calling the
/// browser; a refused permission is `.denied`, and `requestAccess` answers
/// it without prompting; an `http` origin other than `localhost` is
/// `.unsupported`, with `isSupported` false; and once allowed, a chosen
/// device that isn't there is `.notFound` (Chrome's `OverconstrainedError`
/// on `deviceId`), where this Chrome can open its fake devices.
#[test]
fn media_capture_errors_in_chrome() {
    let dir = Scratch::new("media-errors");
    let Some((mut tab, lab)) = capture_lab(&dir, FAKE_DEVICES) else {
        return;
    };
    tab.open(&format!("{}/", lab.origin));
    tab.wait_text("supported");
    lab_click(&mut tab, "Capture", "capture failed: notAsked");
    for name in ["microphone", "camera"] {
        tab.cdp.call(
            "Browser.setPermission",
            json!({ "origin": lab.origin, "permission": { "name": name }, "setting": "denied" }),
        );
    }
    tab.reload();
    tab.wait_text("supported");
    lab_click(&mut tab, "Ask", "access denied");
    lab_click(&mut tab, "Capture", "capture failed: denied");
    let insecure = lab.origin.replace("localhost", "insecure.test");
    tab.open(&format!("{insecure}/"));
    tab.wait_text("unsupported");
    lab_click(&mut tab, "Capture", "capture failed: unsupported");
    for name in ["microphone", "camera"] {
        tab.cdp.call(
            "Browser.setPermission",
            json!({ "origin": lab.origin, "permission": { "name": name }, "setting": "granted" }),
        );
    }
    tab.open(&format!("{}/", lab.origin));
    tab.wait_text("supported");
    if can_capture(&mut tab) {
        lab_click(&mut tab, "Capture", "captured 1 audio");
        lab_click(&mut tab, "Capture missing", "capture failed: notFound");
    }
    drop(tab);
}

/// `.voice`, `.audio` and `.video` each record two seconds of Chrome's fake
/// devices in their WebM form, and the file, played back in a `video`
/// element, decodes to about two seconds (46, "Recording").
#[test]
fn media_records_each_named_format_in_chrome() {
    let dir = Scratch::new("media-record");
    let Some((mut tab, lab)) = capture_lab(&dir, FAKE_MEDIA) else {
        return;
    };
    tab.open(&format!("{}/", lab.origin));
    tab.wait_text("supported");
    if !can_capture(&mut tab) {
        return;
    }
    lab_click(&mut tab, "Ask", "access granted");
    for (button, media) in [
        ("Record voice", "audio/webm"),
        ("Record audio", "audio/webm"),
        ("Record video", "video/webm"),
    ] {
        let status = lab_click(&mut tab, button, "recorded");
        assert_eq!(
            status,
            format!("recorded {media} recording.webm"),
            "{button}"
        );
        // The player renders a moment after the status line. MediaRecorder's
        // WebM has no duration in its header: seeking past the end makes
        // Chrome scan the file and report it.
        let duration = tab.eval(
            "(async () => { for (let i = 0; i < 100 && !document.querySelector('video[aria-label=Playback]'); i++) await new Promise((r) => setTimeout(r, 50)); const v = document.querySelector('video[aria-label=Playback]'); if (v.readyState < 1) await new Promise((r) => v.addEventListener('loadedmetadata', r, { once: true })); if (!Number.isFinite(v.duration)) { v.currentTime = 1e9; await new Promise((r) => v.addEventListener('durationchange', r, { once: true })); } return v.duration; })()",
        );
        let seconds = duration.as_f64().unwrap_or(0.0);
        assert!(
            (1.5..3.5).contains(&seconds),
            "{button}: the recording decodes to {seconds} s"
        );
    }
    drop(tab);
}

/// The memos example (46, "Example apps") under `k dev`: the first click
/// on Record asks (Chrome's fake UI allows), the level meter shows while
/// recording, Stop stores the memo, and its row plays it back from the
/// server's copy.
#[test]
fn memos_records_a_memo_and_plays_it_back() {
    let dir = Scratch::new("memos-flow");
    let Some(mut tab) = page::chrome_with(&dir.0, FAKE_MEDIA) else {
        return;
    };
    copy_example("memos", &dir.0.join("memos"));
    let (_k, port) = Proc::serve(
        &["dev", "."],
        1,
        &dir.0.join("memos"),
        &[],
        &["DATABASE_URL"],
        Duration::from_secs(120),
    );
    tab.open(&format!("http://localhost:{port}/"));
    tab.wait_text("No memos yet.");
    if !can_capture(&mut tab) {
        return;
    }
    let record = tab.role("button", "Record");
    tab.wait_handles_click(&record);
    tab.click(&record);
    tab.role("meter", "Microphone level");
    tab.wait_text("Recording 0:02");
    let stop = tab.role("button", "Stop");
    tab.click(&stop);
    tab.role("button", "Download");
    tab.wait_text("0:02");
    let duration = tab.eval(
        "(async () => { for (let i = 0; i < 100 && !document.querySelector('audio'); i++) await new Promise((r) => setTimeout(r, 50)); const a = document.querySelector('audio'); if (a.readyState < 1) await new Promise((r) => a.addEventListener('loadedmetadata', r, { once: true })); if (!Number.isFinite(a.duration)) { a.currentTime = 1e9; await new Promise((r) => a.addEventListener('durationchange', r, { once: true })); } return a.duration; })()",
    );
    let seconds = duration.as_f64().unwrap_or(0.0);
    assert!(
        (1.5..4.0).contains(&seconds),
        "the memo plays back {seconds} s"
    );
    drop(tab);
}

// ------------------------------------------------------------ calls

/// Chrome's fake camera with every prompt allowed, for calls (46, "Testing
/// the stdlib against real browsers"), with background tabs kept running:
/// the two tabs of a call take turns in front, and Chrome otherwise pauses
/// a background tab's video. The calls send video only: on macOS headless
/// Chrome's fake microphone waits for a system permission the test process
/// may not have (`can_capture`), while the fake camera opens.
const FAKE_CALL: &[&str] = &[
    "--use-fake-device-for-media-stream",
    "--use-fake-ui-for-media-stream",
    "--autoplay-policy=no-user-gesture-required",
    "--disable-background-media-suspend",
    "--disable-renderer-backgrounding",
    "--disable-background-timer-throttling",
    "--disable-backgrounding-occluded-windows",
    // Host candidates as addresses rather than `.local` names: several
    // Chromes of a parallel test run otherwise resolve each other's names.
    "--disable-features=WebRtcHideLocalIpsWithMdns",
];

/// Installed in each tab before the app runs: every `RTCPeerConnection`
/// it makes, in `__pcs`, for a failure message ([`call_ice`]).
const CALL_PCS: &str = r#"(() => {
  const R = RTCPeerConnection;
  const t0 = performance.now();
  window.__pcs = [];
  window.__trace = [];
  const log = (...a) => window.__trace.push(`${Math.round(performance.now() - t0)} ${a.join(" ")}`);
  window.RTCPeerConnection = function (c) {
    const p = new R(c);
    const n = window.__pcs.push(p);
    log(n, "new");
    p.addEventListener("icecandidate", (e) => log(n, "candidate", e.candidate ? e.candidate.candidate.split(" ").slice(4, 8).join(" ") : "end"));
    p.addEventListener("icegatheringstatechange", () => log(n, "gathering", p.iceGatheringState));
    p.addEventListener("negotiationneeded", () => log(n, "negotiationneeded", p.signalingState));
    p.addEventListener("signalingstatechange", () => log(n, "signaling", p.signalingState));
    for (const k of ["setLocalDescription", "setRemoteDescription", "addIceCandidate"]) {
      const f = p[k].bind(p);
      p[k] = (...a) => {
        log(n, k, a[0]?.type ?? (a[0]?.candidate ? "cand" : ""), p.signalingState);
        return f(...a).then((v) => (log(n, k, "ok"), v), (e) => (log(n, k, "failed", e.name), Promise.reject(e)));
      };
    }
    return p;
  };
  window.RTCPeerConnection.prototype = R.prototype;
})();"#;

/// Installed in each tab of the glare and replace tests before the app
/// runs: `__log` records every `setLocalDescription` and
/// `setRemoteDescription` with the signalling state it found, and while
/// `__hold` is set the page's WebSocket frames wait, until `__release()`.
const CALL_HOOKS: &str = r#"(() => {
  const send = WebSocket.prototype.send;
  const held = [];
  window.__hold = false;
  WebSocket.prototype.send = function (d) { if (window.__hold) held.push([this, d]); else send.call(this, d); };
  window.__release = () => { window.__hold = false; for (const [ws, d] of held.splice(0)) send.call(ws, d); };
  window.__log = [];
  const P = RTCPeerConnection.prototype;
  const local = P.setLocalDescription;
  const remote = P.setRemoteDescription;
  P.setLocalDescription = function (...a) { window.__log.push(["local", this.signalingState]); return local.apply(this, a); };
  P.setRemoteDescription = function (d, ...a) { window.__log.push(["remote", d?.type ?? "", this.signalingState]); return remote.call(this, d, ...a); };
})();"#;

/// The calling fixture (`fixtures/calling`) under `k dev` with `env`, and
/// two tabs of one Chrome on it, each with `hooks` installed.
fn call_lab(dir: &Scratch, env: &[(&str, String)], hooks: &str) -> Option<(Tab, Tab, Proc)> {
    let mut a = page::chrome_with(&dir.0, FAKE_CALL)?;
    let fixture =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/calling");
    copy_project(&fixture, &dir.0.join("calling"));
    let (k, port) = Proc::serve(
        &["dev", "."],
        1,
        &dir.0.join("calling"),
        env,
        &["K_TURN_URLS", "K_TURN_SECRET"],
        Duration::from_secs(120),
    );
    let origin = format!("http://localhost:{port}");
    a.before_each_document(CALL_PCS);
    if !hooks.is_empty() {
        a.before_each_document(hooks);
    }
    a.open(&format!("{origin}/"));
    a.wait_text("state: none");
    let mut b = a.new_tab();
    b.before_each_document(CALL_PCS);
    if !hooks.is_empty() {
        b.before_each_document(hooks);
    }
    b.open(&format!("{origin}/"));
    b.wait_text("state: none");
    Some((a, b, k))
}

/// Brings `tab` to the front and clicks its button `name`.
fn call_press(tab: &mut Tab, name: &str) {
    tab.bring_to_front();
    let button = tab.role("button", name);
    tab.wait_handles_click(&button);
    tab.click(&button);
}

/// Waits for `tab`'s status line to start with `expected`.
fn call_status(tab: &mut Tab, expected: &str) -> String {
    tab.wait(&format!("the status to start with {expected:?}"), |t| {
        let status = t.eval("document.querySelector('[role=status]')?.innerText ?? ''");
        let text = status.as_str().unwrap_or_default().to_owned();
        text.starts_with(expected).then_some(text)
    })
}

/// Joins both tabs (`button`: "Join" or "Join relayed") and waits until
/// each is connected to the other.
fn call_join(a: &mut Tab, b: &mut Tab, button: &str) {
    call_press(a, button);
    call_status(a, "joining");
    call_press(b, button);
    call_status(b, "paired");
    call_status(a, "paired");
    let deadline = Instant::now() + Duration::from_secs(60);
    let connected = |t: &mut Tab| {
        t.eval("document.body.innerText")
            .as_str()
            .is_some_and(|x| x.contains("state: connected"))
    };
    while !(connected(a) && connected(b)) {
        if Instant::now() > deadline {
            let a_state = call_ice(a);
            let b_state = call_ice(b);
            let a_trace = a.eval("(window.__trace ?? []).join('\\n')");
            let b_trace = b.eval("(window.__trace ?? []).join('\\n')");
            panic!(
                "the tabs didn't connect within a minute:\n{a_state}\n{b_state}\nA:\n{}\nB:\n{}",
                a_trace.as_str().unwrap_or_default(),
                b_trace.as_str().unwrap_or_default()
            );
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// What `tab`'s connections say about ICE (see [`CALL_PCS`]), for a
/// failure message.
fn call_ice(tab: &mut Tab) -> String {
    let v = tab.eval(
        "(async () => { const out = []; for (const p of window.__pcs ?? []) { out.push(`${p.signalingState} ${p.iceGatheringState} ${p.iceConnectionState}`); (await p.getStats()).forEach((s) => { if (s.type === 'candidate-pair') out.push(`  pair ${s.state} ${s.nominated}`); else if (s.type.endsWith('candidate')) out.push(`  ${s.type} ${s.candidateType} ${s.address}:${s.port} ${s.protocol}`); }); } return out.join('\\n'); })()",
    );
    v.as_str().unwrap_or_default().to_owned()
}

/// Waits until `tab`'s remote picture is `width` pixels wide: the other
/// tab's camera arrived and decodes, at the size it sends.
fn call_remote_width(tab: &mut Tab, width: u64) {
    tab.bring_to_front();
    tab.wait(&format!("the remote picture to be {width} wide"), |t| {
        let w = t.eval("document.querySelector('video[aria-label=Remote]')?.videoWidth ?? 0");
        (w.as_u64() == Some(width)).then_some(())
    });
}

/// `__log`'s entries of `tab` (see [`CALL_HOOKS`]).
fn call_log(tab: &mut Tab) -> Vec<Value> {
    tab.eval("window.__log")
        .as_array()
        .cloned()
        .unwrap_or_default()
}

/// Two tabs on `localhost` call each other through the K server's relay
/// with no ICE servers at all (46, "STUN and TURN without third-party
/// accounts"): host candidates are enough on one machine. One tab's fake
/// camera arrives at the other at the size it sent, `stats()` reports a
/// direct path, and hanging up closes the other tab's side `.byPeer`.
#[test]
fn rtc_two_tabs_call_each_other_without_ice_servers_in_chrome() {
    let dir = Scratch::new("rtc-direct");
    let Some((mut a, mut b, _k)) = call_lab(&dir, &[], "") else {
        return;
    };
    call_join(&mut a, &mut b, "Join");
    call_press(&mut a, "Send 640");
    call_status(&mut a, "sending 640");
    call_remote_width(&mut b, 640);
    call_press(&mut b, "Stats");
    assert_eq!(call_status(&mut b, "stats"), "stats relayed=false");
    call_press(&mut a, "Hang up");
    b.wait_text("state: closed byPeer");
    a.wait_text("state: closed byApp");
    drop(b);
    drop(a);
}

/// A coturn container (`coturn/coturn:4.6`) with the TURN REST secret
/// `secret`, on a UDP port of the tests' range, or
/// `None` without Docker (with `K_REQUIRE_DOCKER=1` that fails instead).
/// Only the listening port is published: both tabs relay, so media goes
/// from one relay port to the other inside the container, whose address
/// coturn advertises. Unlike `examples/call/compose.yaml`, it doesn't deny
/// the private ranges, since that container address is one of them.
struct Coturn {
    name: String,
    /// This machine's address on its network: Chrome talks to a TURN
    /// server only from a network interface, never over loopback.
    host: String,
    port: u16,
}

impl Coturn {
    fn start(secret: &str) -> Option<Coturn> {
        let required = std::env::var("K_REQUIRE_DOCKER").is_ok_and(|v| v == "1");
        let docker = std::process::Command::new("docker")
            .args(["version", "--format", "{{.Server.Version}}"])
            .output()
            .is_ok_and(|o| o.status.success());
        if !docker {
            assert!(
                !required,
                "K_REQUIRE_DOCKER=1, but Docker isn't available to start coturn"
            );
            eprintln!("skipped: no Docker to start coturn (K_REQUIRE_DOCKER=1 makes this fail)");
            return None;
        }
        // The address of the interface that would reach another network;
        // a UDP "connection" sends nothing.
        let host = std::net::UdpSocket::bind("0.0.0.0:0")
            .and_then(|s| s.connect("192.0.2.1:9").and_then(|()| s.local_addr()))
            .map(|a| a.ip())
            .ok()
            .filter(|ip| !ip.is_loopback() && !ip.is_unspecified());
        let Some(host) = host else {
            assert!(
                !required,
                "K_REQUIRE_DOCKER=1, but this machine has no network address for coturn"
            );
            eprintln!("skipped: no network address for Chrome to reach coturn at");
            return None;
        };
        let host = host.to_string();
        let name = format!("k-test-coturn-{}", std::process::id());
        let _ = std::process::Command::new("docker")
            .args(["rm", "--force", &name])
            .output();
        for attempt in 0..20u16 {
            let port = 5200 + ((std::process::id() as u16).wrapping_add(attempt * 41)) % 800;
            let out = std::process::Command::new("docker")
                .args([
                    "run",
                    "--detach",
                    "--rm",
                    "--name",
                    &name,
                    "--publish",
                    // On every interface: Docker Desktop doesn't forward a
                    // port published on 127.0.0.1 alone here (checked
                    // 2026-11). The secret is the test's, and the
                    // container lives only as long as the test.
                    &format!("{port}:3478/udp"),
                    "coturn/coturn:4.6",
                    "-n",
                    "--no-cli",
                    "--fingerprint",
                    "--listening-port=3478",
                    "--min-port=49160",
                    "--max-port=49200",
                    "--realm=k.test",
                    "--use-auth-secret",
                    &format!("--static-auth-secret={secret}"),
                    "--no-multicast-peers",
                    "--log-file=stdout",
                ])
                .output()
                .expect("docker runs");
            if out.status.success() {
                // Ready once it answers a STUN binding request at the address
                // Chrome will use: its log says it listens before the
                // published port reaches it.
                let deadline = Instant::now() + Duration::from_secs(60);
                let socket = std::net::UdpSocket::bind("0.0.0.0:0").expect("a UDP socket");
                socket
                    .set_read_timeout(Some(Duration::from_millis(250)))
                    .expect("a timeout");
                let mut request = vec![0x00, 0x01, 0x00, 0x00, 0x21, 0x12, 0xa4, 0x42];
                request.extend((0..12u8).map(|i| i.wrapping_mul(37)));
                let mut ready = false;
                while !ready && Instant::now() < deadline {
                    let _ = socket.send_to(&request, (host.as_str(), port));
                    let mut answer = [0u8; 512];
                    ready = socket
                        .recv_from(&mut answer)
                        .is_ok_and(|(n, _)| n >= 20 && answer[0] == 0x01);
                }
                assert!(
                    ready,
                    "coturn answers STUN at {host}:{port} within a minute"
                );
                return Some(Coturn { name, host, port });
            }
            let _ = std::process::Command::new("docker")
                .args(["rm", "--force", &name])
                .output();
        }
        panic!("a coturn container starts");
    }
}

impl Drop for Coturn {
    fn drop(&mut self) {
        let _ = std::process::Command::new("docker")
            .args(["rm", "--force", &self.name])
            .output();
    }
}

/// The same call through the deployment's own TURN server: `k dev` with
/// `K_TURN_URLS` naming a coturn container and `K_TURN_SECRET` its
/// `static-auth-secret`, so `IceServers` signs credentials coturn accepts
/// (46, "STUN and TURN without third-party accounts"), and both tabs with
/// `IcePolicy.relayOnly`: they connect, the picture arrives, and `stats()`
/// on both sides reports the relay.
#[test]
fn rtc_call_relays_through_coturn_in_chrome() {
    let secret = "k-test-turn-secret";
    let Some(turn) = Coturn::start(secret) else {
        return;
    };
    let dir = Scratch::new("rtc-relay");
    let env = [
        (
            "K_TURN_URLS",
            format!("turn:{}:{}?transport=udp", turn.host, turn.port),
        ),
        ("K_TURN_SECRET", secret.to_owned()),
    ];
    let Some((mut a, mut b, _k)) = call_lab(&dir, &env, "") else {
        return;
    };
    call_join(&mut a, &mut b, "Join relayed");
    call_press(&mut a, "Send 640");
    call_remote_width(&mut b, 640);
    call_press(&mut b, "Stats");
    assert_eq!(call_status(&mut b, "stats"), "stats relayed=true");
    call_press(&mut a, "Stats");
    assert_eq!(call_status(&mut a, "stats"), "stats relayed=true");
    drop(b);
    drop(a);
    drop(turn);
}

/// Perfect negotiation under forced glare (46, "`PeerConnections` and
/// `PeerConnection`"): while both tabs' WebSocket frames are held, each
/// adds its camera, so each makes an offer; released, each receives the
/// other's offer while it has one of its own. The polite tab rolls back
/// and answers (the one `setRemoteDescription(offer)` in
/// `have-local-offer`), the impolite one ignores the offer, and both
/// pictures arrive.
#[test]
fn rtc_perfect_negotiation_resolves_glare_in_chrome() {
    let dir = Scratch::new("rtc-glare");
    let Some((mut a, mut b, _k)) = call_lab(&dir, &[], CALL_HOOKS) else {
        return;
    };
    call_join(&mut a, &mut b, "Join");
    let before: Vec<usize> = [&mut a, &mut b]
        .into_iter()
        .map(|t| call_log(t).len())
        .collect();
    for t in [&mut a, &mut b] {
        t.eval("window.__hold = true");
    }
    call_press(&mut a, "Send 640");
    call_press(&mut b, "Send 640");
    for (t, n) in [&mut a, &mut b].into_iter().zip(&before) {
        let n = *n;
        t.wait("an offer of its own, held", |t| {
            let log = call_log(t);
            log[n..]
                .iter()
                .any(|e| e[0] == "local" && e[1] == "stable")
                .then_some(())
        });
    }
    for t in [&mut a, &mut b] {
        t.eval("window.__release()");
    }
    call_remote_width(&mut b, 640);
    call_remote_width(&mut a, 640);
    let rolled_back: usize = [&mut a, &mut b]
        .into_iter()
        .zip(&before)
        .map(|(t, n)| {
            call_log(t)[*n..]
                .iter()
                .filter(|e| e[0] == "remote" && e[1] == "offer" && e[2] == "have-local-offer")
                .count()
        })
        .sum();
    assert_eq!(
        rolled_back, 1,
        "exactly the polite tab took the other's offer over its own"
    );
    for t in [&mut a, &mut b] {
        t.wait_text("state: connected");
    }
    drop(b);
    drop(a);
}

/// A mid-call `MediaSender.replace` (46, "`MediaSender`"): the camera sent
/// at 640 wide is replaced by a capture 320 wide, the other tab's picture
/// changes size in the same received track, and neither tab negotiates
/// again.
#[test]
fn rtc_replace_swaps_the_camera_without_renegotiating_in_chrome() {
    let dir = Scratch::new("rtc-replace");
    let Some((mut a, mut b, _k)) = call_lab(&dir, &[], CALL_HOOKS) else {
        return;
    };
    call_join(&mut a, &mut b, "Join");
    call_press(&mut a, "Send 640");
    call_status(&mut a, "sending 640");
    call_remote_width(&mut b, 640);
    let negotiated: Vec<usize> = [&mut a, &mut b]
        .into_iter()
        .map(|t| call_log(t).len())
        .collect();
    call_press(&mut a, "Send 320");
    call_status(&mut a, "replaced with 320");
    call_remote_width(&mut b, 320);
    let after: Vec<usize> = [&mut a, &mut b]
        .into_iter()
        .map(|t| call_log(t).len())
        .collect();
    assert_eq!(after, negotiated, "replace made no new descriptions");
    drop(b);
    drop(a);
}

// The ICE restart after the network drops packets (46, "Testing the
// stdlib against real browsers") needs a network namespace to drop them
// in; macOS has none, so it isn't here. `rtc.test.js` and `examples/call`'s
// tests restart ICE over the in-process engine.

// ==================================================================== push

/// Chrome's flags for the push tests: every host name but the loopback
/// ones fails to resolve, so Chrome's own push client can't reach a
/// vendor's push service (docs/design/50-push.md: no test contacts one),
/// whatever `--disable-background-networking` leaves running.
const NO_PUSH_SERVICE: &str =
    "--host-resolver-rules=MAP * ~NOTFOUND, EXCLUDE localhost, EXCLUDE 127.0.0.1";

/// The `pushing` fixture under `k run` with a VAPID key, and a tab on it
/// with the notification permission granted, as a user's earlier "Allow"
/// would.
fn pushing(dir: &Scratch) -> Option<(Proc, String, Tab)> {
    copy_project(&repo().join("crates/k_cli/tests/fixtures/pushing"), &dir.0);
    let (private, _) = k_server::crypto::new_key_jwks("p256-ecdsa")
        .unwrap()
        .unwrap();
    let (k, port) = Proc::serve(
        &["run"],
        1,
        &dir.0,
        &[("K_PUSH_PRIVATE_KEY", private)],
        &["K_PUSH_PRIVATE_KEY_FILE", "K_PUSH_CONTACT"],
        Duration::from_secs(120),
    );
    let origin = format!("http://localhost:{port}");
    let mut tab = page::chrome_with(&dir.0, &[NO_PUSH_SERVICE])?;
    tab.cdp.call(
        "Browser.grantPermissions",
        json!({ "origin": origin, "permissions": ["notifications"] }),
    );
    Some((k, origin, tab))
}

/// "Notify me" in real Chrome, as far as is local: the click registers
/// `/k-sw.js` and calls `pushManager.subscribe` with `userVisibleOnly` and
/// the server's key (the public half of `K_PUSH_PRIVATE_KEY`, from the
/// fixture's rpc). Completing the subscription needs Chrome's push
/// service, which only Google runs and which this test must not contact.
/// With every other host unresolvable, the Chrome this was built against
/// keeps the subscription waiting (still pending after 8 seconds), so the
/// test checks everything up to the push manager. A Chrome that rejects
/// instead must make the page say `unavailable` (`PushError.unavailable`),
/// and one that subscribes without its push service must return the
/// browser's keys.
#[test]
fn push_subscribe_reaches_the_push_manager_with_the_servers_key_in_chrome() {
    if !have_chrome() {
        return;
    }
    let dir = Scratch::new("push-subscribe");
    let Some((_k, origin, mut tab)) = pushing(&dir) else {
        return;
    };
    tab.cdp.call(
        "Page.addScriptToEvaluateOnNewDocument",
        json!({ "source": "(() => { const calls = globalThis.__kSubscribes = []; const subscribe = PushManager.prototype.subscribe; \
            PushManager.prototype.subscribe = function (o) { const call = { visible: o.userVisibleOnly, key: Array.from(new Uint8Array(o.applicationServerKey)) }; calls.push(call); \
            return subscribe.call(this, o).then((s) => { call.ok = s.toJSON(); return s; }, (e) => { call.error = e.name; throw e; }); }; })();" }),
    );
    tab.open(&format!("{origin}/"));
    let button = tab.role("button", "Notify me");
    tab.click(&button);
    tab.wait("the page calls pushManager.subscribe", |t| {
        t.eval("globalThis.__kSubscribes.length")
            .as_u64()
            .is_some_and(|n| n > 0)
            .then_some(())
    });
    // Chrome may answer at once or keep retrying its push service; give it
    // a few seconds, and treat no answer as "can't finish locally".
    let settled = Instant::now() + Duration::from_secs(8);
    let said = loop {
        let s = tab.eval("document.querySelector('[role=status]')?.textContent ?? ''");
        let s = s.as_str().unwrap_or_default().to_owned();
        if !s.is_empty() || Instant::now() > settled {
            break s;
        }
        std::thread::sleep(Duration::from_millis(100));
    };
    let calls = tab.eval("globalThis.__kSubscribes");
    let call = &calls[0];
    assert_eq!(call["visible"], json!(true), "{calls}");
    let key = tab.eval(
        "fetch('/pushing.alerts.AlertsProtocol/ServerKey', { method: 'POST', headers: { 'content-type': 'application/json', 'connect-protocol-version': '1' }, body: '{}' }).then((r) => r.json()).then((j) => j.value)",
    );
    let key = k_server::crypto::from_base64url(key.as_str().unwrap()).unwrap();
    let sent: Vec<u8> = call["key"]
        .as_array()
        .unwrap()
        .iter()
        .map(|b| u8::try_from(b.as_u64().unwrap()).unwrap())
        .collect();
    assert_eq!(sent, key, "subscribed with the server's key");
    let scope = tab.eval("navigator.serviceWorker.getRegistration('/').then((r) => r && r.active && r.active.scriptURL)");
    assert_eq!(scope, json!(format!("{origin}/k-sw.js")));
    match said.as_str() {
        "" => {
            assert!(call.get("ok").is_none(), "{calls}");
            eprintln!(
                "push subscribe: Chrome's subscribe is still waiting for its push service after 8 s; checked up to the push manager"
            );
        }
        "unavailable" => {
            assert!(call["error"].is_string(), "the browser refused: {calls}");
            eprintln!(
                "push subscribe: Chrome needs its push service to finish subscribing ({}); checked up to the push manager",
                call["error"]
            );
        }
        "on" => {
            assert!(call["ok"]["keys"]["p256dh"].is_string(), "{calls}");
            assert!(call["ok"]["keys"]["auth"].is_string(), "{calls}");
        }
        other => panic!("the page said {other:?}: {calls}"),
    }
    tab.allow_errors();
    drop(tab);
}

/// A push message reaches `k-sw.js` and is shown: the page registers the
/// worker (by trying to subscribe), DevTools delivers the JSON a
/// `WebPush.send` encrypts (`ServiceWorker.deliverPushMessage`, which hands
/// it to the worker's `push` event as the browser does after decrypting),
/// and the worker shows it as the `SystemNotification` it is, with its page
/// for the click. What the worker showed is recorded inside the worker,
/// once Chrome has shown it, rather than read back with
/// `getNotifications()` (see `todos_reminds_through_the_service_worker`).
#[test]
fn push_message_is_shown_by_the_service_worker_in_chrome() {
    if !have_chrome() {
        return;
    }
    let dir = Scratch::new("push-deliver");
    let Some((_k, origin, mut tab)) = pushing(&dir) else {
        return;
    };
    tab.cdp.call("ServiceWorker.enable", json!({}));
    tab.open(&format!("{origin}/"));
    let button = tab.role("button", "Notify me");
    tab.click(&button);
    tab.wait("the service worker is active", |t| {
        (t.eval("navigator.serviceWorker.getRegistration('/').then((r) => !!(r && r.active))")
            == json!(true))
        .then_some(())
    });
    // Chrome's own extensions have service workers too: ours is the one
    // running `/k-sw.js`.
    let script = format!("{origin}/k-sw.js");
    let ours = |v: &Value| {
        v["scriptURL"] == script.as_str()
            && v["runningStatus"] == "running"
            && v["targetId"].is_string()
    };
    let version = tab
        .cdp
        .wait_event_where(
            "ServiceWorker.workerVersionUpdated",
            0,
            Duration::from_secs(30),
            |p| {
                p["versions"]
                    .as_array()
                    .is_some_and(|vs| vs.iter().any(ours))
            },
        )
        .expect("the worker runs");
    let running = version["versions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| ours(v))
        .unwrap()
        .clone();
    let registration = running["registrationId"].as_str().unwrap().to_owned();
    let target = running["targetId"].as_str().unwrap().to_owned();
    let mut worker = cdp::Cdp::connect(&cdp::target_url(tab.devtools_port(), &target)).unwrap();
    worker.call(
        "Runtime.evaluate",
        json!({ "expression": "(() => { const shown = self.__kShown = []; const show = self.registration.showNotification.bind(self.registration); \
            self.registration.showNotification = (title, o) => show(title, o).then(() => { shown.push([title, o.body ?? null, o.tag ?? null, o.data]); }); return true; })()" }),
    );
    let message = json!({ "title": "Ping", "body": "Hello from the server", "tag": "ping", "url": "/reports/7" });
    tab.cdp.call(
        "ServiceWorker.deliverPushMessage",
        json!({ "origin": origin, "registrationId": registration, "data": message.to_string() }),
    );
    let deadline = Instant::now() + page::WAIT;
    let shown = loop {
        let r = worker.call(
            "Runtime.evaluate",
            json!({ "expression": "JSON.stringify(self.__kShown)", "returnByValue": true }),
        );
        let shown: Value =
            serde_json::from_str(r["result"]["value"].as_str().unwrap_or("[]")).unwrap();
        if shown.as_array().is_some_and(|a| !a.is_empty()) {
            break shown;
        }
        assert!(Instant::now() < deadline, "the worker showed nothing: {r}");
        std::thread::sleep(Duration::from_millis(100));
    };
    assert_eq!(
        shown,
        json!([["Ping", "Hello from the server", "ping", { "url": "/reports/7", "from": "/" }]])
    );
    tab.allow_errors();
    drop(worker);
    drop(tab);
}

// ================================================================ passkeys

/// The `passkeys` fixture under `k run` (its public origin
/// `http://localhost:<port>`), and a tab with Chrome's virtual
/// authenticator (docs/design/51-passkeys.md, "Tests"): CTAP2 over the
/// internal transport, with resident keys and user verification, which
/// answers a ceremony without a sheet, as a platform authenticator whose
/// user always agrees.
fn passkeys(dir: &Scratch) -> Option<(Proc, u16, Tab, String)> {
    copy_project(&repo().join("crates/k_cli/tests/fixtures/passkeys"), &dir.0);
    let (k, port) = Proc::serve(&["run"], 1, &dir.0, &[], &[], Duration::from_secs(120));
    let mut tab = page::chrome(&dir.0)?;
    tab.cdp.call("WebAuthn.enable", json!({}));
    let added = tab.cdp.call(
        "WebAuthn.addVirtualAuthenticator",
        json!({ "options": {
            "protocol": "ctap2",
            "transport": "internal",
            "hasResidentKey": true,
            "hasUserVerification": true,
            "isUserVerified": true,
            "automaticPresenceSimulation": true,
        } }),
    );
    let id = added["authenticatorId"].as_str()?.to_owned();
    Some((k, port, tab, id))
}

/// Waits for the panel's status to say `text`, and returns what it says.
fn passkey_status(tab: &mut Tab, text: &str) -> String {
    tab.wait(&format!("the status says {text:?}"), |t| {
        let s = t.eval("document.querySelector('[role=status]')?.textContent ?? ''");
        let s = s.as_str().unwrap_or_default().to_owned();
        (s == text).then_some(s)
    })
}

/// A passkey made, signed out, and signed in with in real Chrome: the
/// clicks run `navigator.credentials.create` and `get` against the virtual
/// authenticator, the server verifies the registration and the ES256
/// assertion, and the authenticator holds one resident credential for
/// `localhost` whose counter the sign-in advanced.
#[test]
fn passkeys_sign_up_and_in_with_a_virtual_authenticator_in_chrome() {
    if !have_chrome() {
        return;
    }
    let dir = Scratch::new("passkeys-sign-in");
    let Some((_k, port, mut tab, authenticator)) = passkeys(&dir) else {
        return;
    };
    tab.open(&format!("http://localhost:{port}/"));
    passkey_status(&mut tab, "ready");
    let name = tab.role("textbox", "Name");
    tab.fill(&name, "ada");
    let up = tab.role("button", "Sign up with a passkey");
    tab.click(&up);
    passkey_status(&mut tab, "signed up");
    let held = tab.cdp.call(
        "WebAuthn.getCredentials",
        json!({ "authenticatorId": authenticator }),
    );
    let creds = held["credentials"].as_array().cloned().unwrap_or_default();
    assert_eq!(creds.len(), 1, "{held}");
    assert_eq!(creds[0]["isResidentCredential"], json!(true), "{held}");
    assert_eq!(creds[0]["rpId"], json!("localhost"), "{held}");
    let out = tab.role("button", "Sign out");
    tab.click(&out);
    passkey_status(&mut tab, "signed out");
    let inn = tab.role("button", "Sign in with a passkey");
    tab.click(&inn);
    passkey_status(&mut tab, "signed in as ada");
    let after = tab.cdp.call(
        "WebAuthn.getCredentials",
        json!({ "authenticatorId": authenticator }),
    );
    assert!(
        after["credentials"][0]["signCount"].as_u64().unwrap_or(0) >= 1,
        "the sign-in counted: {after}"
    );
    drop(tab);
}

/// The same page opened on another origin (`127.0.0.1` while the public
/// origin is `localhost`), as a phishing copy would be: the server's
/// options name the RP id `localhost`, which a page on `127.0.0.1` can't
/// use, so Chrome refuses the ceremony and the authenticator makes no
/// credential. The server's own origin check (an assertion whose client
/// data names another origin) is `k test`'s and `k_server::passkey`'s.
#[test]
fn passkeys_refuse_a_page_on_another_origin_in_chrome() {
    if !have_chrome() {
        return;
    }
    let dir = Scratch::new("passkeys-origin");
    let Some((_k, port, mut tab, authenticator)) = passkeys(&dir) else {
        return;
    };
    tab.open(&format!("http://127.0.0.1:{port}/"));
    passkey_status(&mut tab, "ready");
    let name = tab.role("textbox", "Name");
    tab.fill(&name, "eve");
    let up = tab.role("button", "Sign up with a passkey");
    tab.click(&up);
    let said = tab.wait("the sign-up ends", |t| {
        let s = t.eval("document.querySelector('[role=status]')?.textContent ?? ''");
        let s = s.as_str().unwrap_or_default().to_owned();
        (s != "ready").then_some(s)
    });
    assert_eq!(
        said, "failed",
        "Chrome refuses an RP id the origin can't use"
    );
    let held = tab.cdp.call(
        "WebAuthn.getCredentials",
        json!({ "authenticatorId": authenticator }),
    );
    assert_eq!(
        held["credentials"].as_array().map_or(0, Vec::len),
        0,
        "{held}"
    );
    tab.allow_errors();
    drop(tab);
}

// ============================================================ collections

/// Builds the `collections` fixture copied to `<dir>/project` (with
/// `Topics.k.v2` as `Topics.k` when `v2`) into `<dir>/dist`.
fn build_notebook(dir: &Scratch, v2: bool) -> std::path::PathBuf {
    let project = dir.0.join("project");
    if !project.exists() {
        copy_project(
            &repo().join("crates/k_cli/tests/fixtures/collections"),
            &project,
        );
    }
    if v2 {
        let topics = project.join("src/notebook");
        std::fs::copy(topics.join("Topics.k.v2"), topics.join("Topics.k")).unwrap();
    }
    let dist = dir.0.join("dist");
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_k"))
        .arg("build")
        .arg(&project)
        .arg("--out")
        .arg(&dist)
        .output()
        .expect("k build runs");
    assert!(
        out.status.success(),
        "k build: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    dist
}

/// Writes `text` as an entry in the notebook and waits for the count.
fn write_entry(tab: &mut Tab, text: &str, count: usize) {
    let field = tab.role("textbox", "Entry");
    tab.fill(&field, text);
    let write = tab.role("button", "Write");
    tab.wait_handles_click(&write);
    tab.click(&write);
    tab.wait_text(&format!("{count} entries"));
}

/// The version of the notebook's IndexedDB database, as the page sees it.
fn notebook_version(tab: &mut Tab) -> Value {
    tab.eval(
        "new Promise((resolve) => { const r = indexedDB.open('k-collections:notebook'); r.onsuccess = () => { const v = r.result.version; r.result.close(); resolve(v); }; r.onerror = () => resolve(null); })",
    )
}

/// Indexed collections in real Chrome (52-indexed-collections.md): the
/// entries are in the app's own IndexedDB database and still there after
/// a reload, newest first through the `writtenAt` index.
#[test]
fn collections_persist_across_a_reload_in_chrome() {
    if !have_chrome() {
        return;
    }
    let dir = Scratch::new("collections-reload");
    let dist = build_notebook(&dir, false);
    let site = StaticSite::serve(&dist);
    let mut tab = tab!(dir);
    tab.open(&format!("{}/", site.origin));
    tab.wait_text("0 entries");
    write_entry(&mut tab, "first", 1);
    write_entry(&mut tab, "second", 2);
    tab.reload();
    tab.wait_text("2 entries");
    let text = tab.visible_text();
    let (second, first) = (text.find("second"), text.find("first"));
    assert!(
        second.is_some() && first.is_some() && second < first,
        "newest first after the reload: {text}"
    );
    let names = tab.eval("indexedDB.databases().then((d) => d.map((x) => x.name))");
    assert_eq!(
        names,
        json!(["k-collections:notebook"]),
        "one database, namespaced"
    );
    assert_eq!(notebook_version(&mut tab), json!(1));
}

/// A build that adds an index upgrades the database the previous build
/// left (52-indexed-collections.md, "Versions and upgrades"): the new
/// `topic` index finds the entries stored before it existed, at the next
/// version.
#[test]
fn collections_upgrade_adds_an_index_over_stored_values_in_chrome() {
    if !have_chrome() {
        return;
    }
    let dir = Scratch::new("collections-upgrade");
    let dist = build_notebook(&dir, false);
    let site = StaticSite::serve(&dist);
    let mut tab = tab!(dir);
    tab.open(&format!("{}/", site.origin));
    tab.wait_text("0 entries");
    write_entry(&mut tab, "#tagged one", 1);
    write_entry(&mut tab, "plain", 2);
    assert!(tab.has_no_role("button", "Find tagged"));
    build_notebook(&dir, true);
    tab.reload();
    tab.wait_text("2 entries");
    let find = tab.role("button", "Find tagged");
    tab.wait_handles_click(&find);
    tab.click(&find);
    tab.wait_text("Found: #tagged one");
    assert_eq!(notebook_version(&mut tab), json!(2), "one upgrade");
}

/// Background tabs at a foreground tab's priority, for a test whose
/// background tab must answer promptly. The newer tab's upgrade waits until
/// the older tab, in the background, has handled `versionchange` and closed
/// its connection; Chrome runs a background renderer at a low process
/// priority, and on a loaded machine (a parallel test run) that took more
/// than 20 s, past [`page::WAIT`]'s margin, while the app did nothing wrong.
const BACKGROUND_TABS_RUN: &[&str] = &[
    "--disable-renderer-backgrounding",
    "--disable-background-timer-throttling",
    "--disable-backgrounding-occluded-windows",
];

/// Two tabs of two builds on one database: the newer tab's upgrade isn't
/// blocked by the older tab, which closes its connection when asked and
/// reopens it on its next call without upgrading again, so both keep
/// reading and writing the same entries.
#[test]
fn collections_share_one_database_across_two_builds_in_two_tabs_in_chrome() {
    if !have_chrome() {
        return;
    }
    let dir = Scratch::new("collections-tabs");
    let dist = build_notebook(&dir, false);
    let site = StaticSite::serve(&dist);
    let Some(mut older) = page::chrome_with(&dir.0, BACKGROUND_TABS_RUN) else {
        return;
    };
    older.open(&format!("{}/", site.origin));
    older.wait_text("0 entries");
    write_entry(&mut older, "#a", 1);
    build_notebook(&dir, true);
    {
        let mut newer = older.new_tab();
        newer.open(&format!("{}/", site.origin));
        newer.wait_text("1 entries");
        let find = newer.role("button", "Find tagged");
        newer.wait_handles_click(&find);
        newer.click(&find);
        newer.wait_text("Found: #a");
        assert_eq!(notebook_version(&mut newer), json!(2));
        older.bring_to_front();
        write_entry(&mut older, "#b", 2);
        newer.bring_to_front();
        let find = newer.role("button", "Find tagged");
        newer.click(&find);
        newer.wait_text("Found: #a, #b");
        assert_eq!(
            notebook_version(&mut newer),
            json!(2),
            "the older tab didn't upgrade again"
        );
    }
    older.bring_to_front();
    let refresh = older.role("button", "Refresh");
    older.click(&refresh);
    older.wait_text("2 entries");
    assert!(
        !older.visible_text().contains("Problem"),
        "the older tab kept working: {}",
        older.visible_text()
    );
}

// ============================================================ generics

/// One generic `Picker<T>` placed twice in the `generics` fixture (its type
/// argument inferred for fruits, written out for quantities), built with
/// `k build` and served statically: clicking in each updates the page
/// through the injected services' generic members, and the injected
/// `Ranker`'s generic `best` names the dearest fruit.
#[test]
fn a_generic_component_renders_and_emits_its_type_in_chrome() {
    if !have_chrome() {
        return;
    }
    let dir = Scratch::new("generics");
    let project = dir.0.join("project");
    copy_project(
        &repo().join("crates/k_cli/tests/fixtures/generics"),
        &project,
    );
    let dist = dir.0.join("dist");
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_k"))
        .arg("build")
        .arg(&project)
        .arg("--out")
        .arg(&dist)
        .output()
        .expect("k build runs");
    assert!(
        out.status.success(),
        "k build: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let site = StaticSite::serve(&dist);
    let Some(mut tab) = page::chrome(&dir.0) else {
        return;
    };
    tab.open(&format!("{}/", site.origin));
    tab.role("heading", "Fruit");
    tab.role("heading", "Quantity");
    tab.wait_text("Dearest: Mango");
    let mango = tab.role("button", "Mango");
    tab.click(&mango);
    tab.wait_text("Picked Mango × 0");
    let three = tab.role("button", "3 pieces");
    tab.click(&three);
    tab.wait_text("Picked Mango × 3");
    tab.wait_text("Kept 2");
}

// ============================================================ focusing

/// The `focusing` fixture (element references, 53-element-references.md),
/// built with `k build` into `dir` and served statically, and a tab on it.
fn focusing(dir: &Scratch) -> Option<(StaticSite, Tab)> {
    let project = dir.0.join("project");
    copy_project(
        &repo().join("crates/k_cli/tests/fixtures/focusing"),
        &project,
    );
    let dist = dir.0.join("dist");
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_k"))
        .arg("build")
        .arg(&project)
        .arg("--out")
        .arg(&dist)
        .output()
        .expect("k build runs");
    assert!(
        out.status.success(),
        "k build: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let site = StaticSite::serve(&dist);
    let mut tab = page::chrome(&dir.0)?;
    tab.open(&format!("{}/", site.origin));
    tab.role("textbox", "Note");
    Some((site, tab))
}

/// Clicking "Add note" moves the focus off the button and back into the
/// field the handler names with `ElementRef.focus()`.
#[test]
fn element_refs_move_the_focus_after_an_action_in_chrome() {
    if !have_chrome() {
        return;
    }
    let dir = Scratch::new("focusing-action");
    let Some((_site, mut tab)) = focusing(&dir) else {
        return;
    };
    let field = tab.role("textbox", "Note");
    tab.fill(&field, "Buy milk");
    let add = tab.role("button", "Add note");
    tab.click(&add);
    tab.wait_text("Buy milk");
    tab.wait("the field has the focus", |t| {
        t.is_focused(&field).then_some(())
    });
}

/// A refused `@Form` submit puts the focus in its first invalid field,
/// which is marked `aria-invalid` (53-element-references.md, "Focus the
/// framework moves").
#[test]
fn a_failed_submit_focuses_the_first_invalid_field_in_chrome() {
    if !have_chrome() {
        return;
    }
    let dir = Scratch::new("focusing-form");
    let Some((_site, mut tab)) = focusing(&dir) else {
        return;
    };
    let message = tab.role("textbox", "Message");
    tab.fill(&message, "Hello");
    let send = tab.role("button", "Send");
    tab.click(&send);
    let name = tab.role("textbox", "Name");
    tab.wait("the name field has the focus", |t| {
        t.is_focused(&name).then_some(())
    });
    assert_eq!(
        tab.on(
            &name,
            "function() { return this.getAttribute('aria-invalid'); }",
            &[]
        ),
        json!("true")
    );
    tab.wait_text("Sent 0");
}

/// "Jump to the end" scrolls the last heading, far below the fold, into
/// the viewport and focuses it, without making it a Tab stop.
#[test]
fn scroll_into_view_reaches_an_element_in_chrome() {
    if !have_chrome() {
        return;
    }
    let dir = Scratch::new("focusing-scroll");
    let Some((_site, mut tab)) = focusing(&dir) else {
        return;
    };
    let end = tab.role("heading", "The end");
    let inside = "function() { const r = this.getBoundingClientRect(); return r.top >= 0 && r.bottom <= innerHeight; }";
    assert_eq!(
        tab.on(&end, inside, &[]),
        json!(false),
        "below the fold at first"
    );
    let jump = tab.role("button", "Jump to the end");
    tab.click(&jump);
    tab.wait("the end is in view", |t| {
        (t.on(&end, inside, &[]) == json!(true)).then_some(())
    });
    assert!(
        tab.is_focused(&end),
        "the heading has the focus, not {}",
        tab.focused()
    );
    assert_eq!(
        tab.on(
            &end,
            "function() { return this.getAttribute('tabindex'); }",
            &[]
        ),
        json!("-1")
    );
}

// ============================================================ navigating

/// The `navigating` fixture (the router's focus and scroll, text typed
/// before hydration), served server-rendered by `k dev`, and its origin.
fn navigating(dir: &Scratch) -> (Proc, String) {
    let root = dir.0.join("navigating");
    copy_project(
        &repo().join("crates/k_cli/tests/fixtures/navigating"),
        &root,
    );
    let (k, port) = Proc::serve(
        &["dev", "."],
        1,
        &root,
        &[],
        &["DATABASE_URL"],
        Duration::from_secs(120),
    );
    (k, format!("http://localhost:{port}"))
}

/// A navigation moves the focus to the new page's `h1` (made focusable
/// with `tabindex="-1"`) and starts the page at the top; Back shows the
/// start page where it was scrolled when it was left, Forward the other
/// page at its top (53-element-references.md, "Focus the framework moves").
#[test]
fn navigation_focuses_the_heading_and_restores_the_scroll_in_chrome() {
    let dir = Scratch::new("navigating-scroll");
    let mut tab = tab!(dir);
    let (k, origin) = navigating(&dir);
    tab.open(&format!("{origin}/"));
    let link = tab.role("link", "About");
    // The router handles link clicks on its container; it is running once
    // it has taken over the scroll restoration.
    tab.wait_js(
        "history.scrollRestoration === 'manual'",
        "the router running",
    );
    tab.on(
        &link,
        "function() { this.scrollIntoView({ block: 'center' }); }",
        &[],
    );
    let left = tab.eval("scrollY").as_f64().unwrap_or(0.0);
    assert!(left > 1000.0, "the link is far down: {left}");
    tab.click(&link);
    let about = tab.role("heading", "About");
    tab.wait("the heading has the focus", |t| {
        t.is_focused(&about).then_some(())
    });
    assert_eq!(
        tab.on(
            &about,
            "function() { return this.getAttribute('tabindex'); }",
            &[]
        ),
        json!("-1")
    );
    assert_eq!(
        tab.eval("scrollY"),
        json!(0),
        "the new page starts at the top"
    );
    assert_eq!(tab.path(), "/about");
    tab.eval("history.back()");
    let home = tab.role("heading", "Home");
    tab.wait("the start page back where it was", |t| {
        let y = t.eval("scrollY").as_f64().unwrap_or(0.0);
        ((y - left).abs() < 2.0).then_some(())
    });
    assert!(tab.is_focused(&home), "the focus is on {}", tab.focused());
    tab.eval("history.forward()");
    tab.role("heading", "About");
    tab.wait("the other page at its top", |t| {
        (t.eval("scrollY") == json!(0)).then_some(())
    });
    drop(k);
}

/// Text typed into a server-rendered field bound with a plain `bind`,
/// before the page's scripts ran (a slow load), is kept by hydration and
/// reaches the state, as if typed then (15-forms-and-validation.md).
#[test]
fn text_typed_before_hydration_is_kept_in_chrome() {
    let dir = Scratch::new("navigating-typed");
    let mut tab = tab!(dir);
    let (k, origin) = navigating(&dir);
    // The app's entry doesn't load: the page is the server's markup only.
    tab.cdp.call("Network.enable", json!({}));
    tab.cdp.call(
        "Network.setBlockedURLs",
        json!({ "urls": ["*/assets/app.*"] }),
    );
    tab.open(&format!("{origin}/"));
    let field = tab.role("textbox", "Note");
    tab.fill(&field, "typed early");
    assert_eq!(
        tab.eval("document.body.innerText.includes('Note: typed early')"),
        json!(false)
    );
    // Now the scripts run (the module again, under another URL: the
    // browser remembers the blocked one as failed).
    tab.cdp
        .call("Network.setBlockedURLs", json!({ "urls": [] }));
    tab.eval(
        "(() => { const s = document.createElement('script'); s.type = 'module'; \
         s.src = document.querySelector('script[type=module]').src + '?late'; document.body.append(s); })()",
    );
    tab.wait_text("Note: typed early");
    assert_eq!(
        tab.on(&field, "function() { return this.value; }", &[]),
        json!("typed early"),
        "hydration kept the typed text"
    );
    drop(k);
}
