/**
 * @module k:runtime/charts
 *
 * `k.charts.LineChart` (docs/design/37-charts.md; docs/compiler/runtime.md
 * §12.2). One function draws a chart as HTML, an SVG with `role="img"`
 * and the chart's description plus a visually hidden data table, so the
 * server's render and the browser's are the same text. The server writes
 * it inside `<div class="k-chart" data-k-chart="key">`, the key being a
 * hash of that text; in the browser the component claims the server's
 * element while hydrating when its own drawing has the same key, and
 * draws again whenever the chart's data changes.
 *
 * Shipped as its own client chunk, only to apps that use `k.charts`
 * (codegen.md §16), and measured against its own size budget.
 */

import { claimRaw } from "./hydrate.js";
import { renderEffect } from "./signals.js";
import { Instant, localTimeZone } from "./std.js";

/** The drawing's width in SVG units; the SVG scales to its container. */
const WIDTH = 640;
/** Room for the y labels, the right edge, the top and the x labels. */
const LEFT = 48;
const RIGHT = 16;
const TOP = 16;
const BOTTOM = 36;
/** Styles of the hidden table: `VisuallyHidden`'s (codegen.md §10). */
const HIDDEN = "position: absolute; width: 1px; height: 1px; padding: 0; margin: 0; overflow: hidden; clip: rect(0, 0, 0, 0); white-space: nowrap; border: 0";
/** Styles of axis labels and titles: the `chartAxis` and `chartLabel` tokens. */
const AXIS = "fill: var(--k-color-chart-axis); font-size: var(--k-font-size-chart-label)";

/** Escapes text and attribute values. */
const esc = (s) => String(s).replaceAll("&", "&amp;").replaceAll("<", "&lt;").replaceAll(">", "&gt;").replaceAll('"', "&quot;");

/** A number as a label: at most two decimals, never `-0`. */
const num = (n) => String(Math.round(n * 100) / 100 || 0);

/** A position in SVG units, rounded to a tenth. */
const pos = (n) => String(Math.round(n * 10) / 10);

/** An x value as a number on its scale: a `Date`'s milliseconds. */
const xOf = (x) => (x instanceof Instant ? x.ms : Number(x));

/**
 * A formatter of `Date` x values in `zone`, as an hour (`"12:00"`) or, for
 * the table, a day and an hour (`"Jan 15, 12:00"`).
 * @param {string} zone
 * @param {boolean} day
 * @returns {(ms: number) => string}
 */
function dates(zone, day) {
  const f = new Intl.DateTimeFormat("en", { timeZone: zone, hour: "2-digit", minute: "2-digit", hourCycle: "h23", ...(day ? { month: "short", day: "numeric" } : {}) });
  return (ms) => f.format(new Date(ms));
}

/**
 * Ticks for `lo...hi`: about five round steps (1, 2 or 5 × 10ⁿ), and the
 * domain widened to whole steps unless it is `fixed`.
 * @returns {{lo: number, hi: number, ticks: number[]}}
 */
function scale(lo, hi, fixed) {
  if (!(hi > lo)) [lo, hi] = [lo - 1, hi + 1];
  const raw = (hi - lo) / 4;
  const mag = 10 ** Math.floor(Math.log10(raw));
  const step = [1, 2, 5, 10].find((m) => m * mag >= raw) * mag;
  if (!fixed) [lo, hi] = [Math.floor(lo / step) * step, Math.ceil(hi / step) * step];
  const ticks = [];
  for (let t = Math.ceil(lo / step - 1e-9) * step; t <= hi + step * 1e-9; t += step) ticks.push(t);
  return { lo, hi, ticks };
}

/**
 * Draws a line chart as HTML: the SVG, the legend (unless `legend` is
 * `false`) and the hidden data table. The same chart and zone always give
 * the same text.
 * @param {{series: Array<{name: string, points: Array<{x: unknown, y: number}>, color: string|null, dashed: boolean, showsPoints: boolean}>, description: string, height?: string, yAxisTitle?: string|null, xAxisTitle?: string|null, yRange?: {lo: number, hi: number}|null, legend?: boolean}} c
 * @param {string} zone the IANA zone `Date` values are written in
 * @returns {string}
 */
export function lineChart(c, zone) {
  const height = parseFloat(c.height ?? "") || 320;
  const series = c.series ?? [];
  const points = series.flatMap((s) => s.points);
  const isDate = points.length > 0 && points[0].x instanceof Instant;
  const xs = [...new Set(points.map((p) => xOf(p.x)))].sort((a, b) => a - b);
  const ys = points.map((p) => p.y);
  const range = c.yRange ?? null;
  const y = range ? scale(range.lo, range.hi, true) : ys.length ? scale(Math.min(...ys), Math.max(...ys), false) : scale(0, 1, false);
  const x0 = xs[0] ?? 0;
  const x1 = xs.length > 1 ? xs[xs.length - 1] : x0 + 1;
  const px = (v) => LEFT + ((v - x0) / (x1 - x0)) * (WIDTH - LEFT - RIGHT);
  const py = (v) => height - BOTTOM - ((v - y.lo) / (y.hi - y.lo)) * (height - TOP - BOTTOM);
  const label = isDate ? dates(zone, false) : num;
  const color = (s, i) => s.color ?? `var(--k-color-chart-series${(i % 6) + 1})`;
  let svg = `<svg role="img" aria-label="${esc(c.description)}" viewBox="0 0 ${WIDTH} ${pos(height)}" width="100%" height="${pos(height)}">`;
  for (const t of y.ticks) {
    svg += `<line x1="${LEFT}" x2="${WIDTH - RIGHT}" y1="${pos(py(t))}" y2="${pos(py(t))}" style="stroke: var(--k-color-chart-grid)"></line>`;
    svg += `<text x="${LEFT - 6}" y="${pos(py(t) + 4)}" text-anchor="end" style="${AXIS}">${esc(num(t))}</text>`;
  }
  const n = xs.length;
  const shown = n <= 6 ? xs : [0, 1, 2, 3, 4, 5].map((i) => xs[Math.round((i * (n - 1)) / 5)]);
  for (const v of shown) svg += `<text x="${pos(px(v))}" y="${pos(height - BOTTOM + 16)}" text-anchor="middle" style="${AXIS}">${esc(label(v))}</text>`;
  if (c.yAxisTitle != null) svg += `<text x="${pos(-height / 2)}" y="12" transform="rotate(-90)" text-anchor="middle" style="${AXIS}">${esc(c.yAxisTitle)}</text>`;
  if (c.xAxisTitle != null) svg += `<text x="${pos(WIDTH / 2)}" y="${pos(height - 4)}" text-anchor="middle" style="${AXIS}">${esc(c.xAxisTitle)}</text>`;
  series.forEach((s, i) => {
    const sorted = [...s.points].sort((a, b) => xOf(a.x) - xOf(b.x));
    const d = sorted.map((p, j) => `${j ? "L" : "M"}${pos(px(xOf(p.x)))} ${pos(py(p.y))}`).join(" ");
    if (d) svg += `<path d="${d}" fill="none" style="stroke: ${esc(color(s, i))}; stroke-width: 2"${s.dashed ? ' stroke-dasharray="6 4"' : ""}></path>`;
    if (s.showsPoints) for (const p of sorted) svg += `<circle cx="${pos(px(xOf(p.x)))}" cy="${pos(py(p.y))}" r="3" style="fill: ${esc(color(s, i))}"></circle>`;
  });
  svg += "</svg>";
  if (c.legend !== false && series.length) {
    svg += '<ul style="display: flex; flex-wrap: wrap; gap: 1rem; margin: 0.5rem 0 0; padding: 0; list-style: none; font-size: var(--k-font-size-chart-label)">';
    series.forEach((s, i) => {
      svg += `<li><span aria-hidden="true" style="display: inline-block; width: 1rem; height: 0.125rem; margin-right: 0.25rem; vertical-align: middle; background: ${esc(color(s, i))}"></span>${esc(s.name)}</li>`;
    });
    svg += "</ul>";
  }
  const row = isDate ? dates(zone, true) : num;
  let table = `<table style="${HIDDEN}"><caption>${esc(c.description)}</caption><thead><tr><th scope="col">${esc(c.xAxisTitle ?? "")}</th>`;
  for (const s of series) table += `<th scope="col">${esc(s.name)}</th>`;
  table += "</tr></thead><tbody>";
  for (const v of xs) {
    table += `<tr><th scope="row">${esc(row(v))}</th>`;
    for (const s of series) {
      const p = s.points.find((q) => xOf(q.x) === v);
      table += `<td>${p ? esc(num(p.y)) : ""}</td>`;
    }
    table += "</tr>";
  }
  return `${svg}${table}</tbody></table>`;
}

/**
 * The key of a drawing: a 32-bit FNV-1a hash of its text, in base 36.
 * @param {string} html
 * @returns {string}
 */
export function chartKey(html) {
  let h = 0x811c9dc5;
  for (let i = 0; i < html.length; i++) h = Math.imul(h ^ html.charCodeAt(i), 16777619);
  return (h >>> 0).toString(36);
}

/**
 * The chart's props as values, with the defaults `LineChart` declares
 * (`timeZone` aside, which each side resolves).
 * @param {Record<string, () => unknown>} props generated getters
 * @returns {object}
 */
export function chartProps(props) {
  const v = (name) => (props[name] ? props[name]() : undefined);
  return { series: v("series"), description: v("description"), height: v("height"), yAxisTitle: v("yAxisTitle"), xAxisTitle: v("xAxisTitle"), yRange: v("yRange"), legend: v("legend") };
}

/**
 * The browser's `LineChart` component: a `div.k-chart` holding the
 * drawing, drawn again when the props it read change. `timeZone` defaults
 * to the browser's zone (`TimeZone.local`).
 * @param {Record<string, () => unknown>} props
 * @param {object} _ctx
 * @returns {Element}
 */
export function LineChart(props, _ctx) {
  const el = claimRaw("div") ?? document.createElement("div");
  renderEffect(() => {
    const html = lineChart(chartProps(props), props.timeZone?.() ?? localTimeZone());
    const key = chartKey(html);
    if (el.getAttribute("data-k-chart") === key) return;
    el.setAttribute("class", "k-chart");
    el.setAttribute("data-k-chart", key);
    const t = document.createElement("template");
    t.innerHTML = html;
    while (el.firstChild) el.removeChild(el.firstChild);
    el.appendChild(t.content);
  });
  return el;
}
