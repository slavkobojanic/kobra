// The version switcher and the `/next/` banner of the versioned docs site
// (docs/design/48-releases-and-versioning.md, "Docs versioning"). The site
// is `<root>/0.N/…`, `<root>/next/…` and `<root>/versions.json`, built by
// `cargo xtask docs build`. A book built on its own (`mdbook build
// docs/book`, `mdbook serve`) isn't in a version directory, so this does
// nothing there.

"use strict";

(function () {
  if (typeof path_to_root !== "string") {
    return;
  }
  const bookRoot = new URL(path_to_root || "./", location.href);
  const current = bookRoot.pathname.replace(/\/$/, "").split("/").pop();
  if (current !== "next" && !/^\d+\.\d+$/.test(current)) {
    return;
  }
  const siteRoot = new URL("../", bookRoot);
  // This page's path inside the book, so the switcher keeps the reader on
  // the same page in another version.
  const page = location.href.startsWith(bookRoot.href) ? location.href.slice(bookRoot.href.length) : "";

  fetch(new URL("versions.json", siteRoot))
    .then((r) => (r.ok ? r.json() : null))
    .then((versions) => {
      if (!versions) {
        return;
      }
      const live = versions.live || [];
      const choices = live.map((v) => ({ id: v, label: v === versions.latest ? `${v} (latest)` : v }));
      if (versions.next) {
        choices.push({ id: "next", label: "next (unreleased)" });
      }
      if (!choices.some((c) => c.id === current)) {
        choices.push({ id: current, label: `${current} (archived)` });
      }

      const select = document.createElement("select");
      select.className = "k-version";
      select.setAttribute("aria-label", "Documentation version");
      for (const c of choices) {
        const option = document.createElement("option");
        option.value = c.id;
        option.textContent = c.label;
        option.selected = c.id === current;
        select.appendChild(option);
      }
      select.addEventListener("change", () => {
        location.href = new URL(`${select.value}/${page}`, siteRoot).href;
      });
      const bar = document.querySelector("#menu-bar .right-buttons") || document.querySelector("#menu-bar");
      if (bar) {
        bar.prepend(select);
      }

      let banner = null;
      if (current === "next") {
        banner = "This describes unreleased work on main.";
      } else if (!live.includes(current)) {
        banner = `This is the documentation of K ${current}, which is no longer current.`;
      }
      const main = document.querySelector("main");
      if (banner && main) {
        const note = document.createElement("div");
        note.className = "k-version-banner";
        note.setAttribute("role", "note");
        note.textContent = `${banner} `;
        if (versions.latest) {
          const link = document.createElement("a");
          link.href = new URL(`${versions.latest}/${page}`, siteRoot).href;
          link.textContent = `Read the docs of K ${versions.latest}.`;
          note.appendChild(link);
        }
        main.prepend(note);
      }
    })
    .catch(() => {});
})();
