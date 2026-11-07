/**
 * @module k:/tests/charts.test.js
 *
 * `k.charts.LineChart` (docs/design/37-charts.md; docs/compiler/runtime.md
 * §12.2): one HTML rendering shared by the server and the browser, an SVG
 * with the required description plus a hidden data table, `Date` axes
 * labelled in the chart's time zone, and series colors that are theme
 * tokens. In the browser the component claims the server's chart when it
 * is the same chart, and renders it again when its data changes.
 */
import { assert, assertEq, test } from "k:testing";
import { LineChart, chartKey, lineChart } from "k:/runtime/charts.js";
import { hydrate, signal, flush, instantFromSeconds, closedRange } from "k:runtime";

/** The instant `h` hours after 2026-01-15T11:00:00Z, the series' first point. */
const hour = (h) => instantFromSeconds(1768474800 + h * 3600);
const temperature = { name: "Temperature", points: [0, 1, 2, 3].map((h) => ({ x: hour(h), y: [3.4, 4.1, 4.6, 4][h] })), color: "var(--k-color-chart-temperature)", dashed: false, showsPoints: false };
const feelsLike = { name: "Feels like", points: [0, 1, 2, 3].map((h) => ({ x: hour(h), y: [0.6, 1.2, 1.9, 1.1][h] })), color: null, dashed: true, showsPoints: true };
/** The props of a `lineChart` with both series, overridden by `over`. */
const chart = (over = {}) => ({ series: [temperature, feelsLike], description: "Temperatures & more", ...over });

test("the chart is an SVG image described by its description", () => {
  const html = lineChart(chart(), "UTC");
  assert(html.includes('role="img"'), html);
  assert(html.includes('aria-label="Temperatures &amp; more"'), "the description, escaped");
  assert(html.startsWith("<svg"), html.slice(0, 40));
});

test("Date ticks and table rows are written in the chart's time zone", () => {
  const berlin = lineChart(chart(), "Europe/Berlin");
  assert(berlin.includes(">12:00<") && berlin.includes(">15:00<"), berlin);
  assert(!berlin.includes(">11:00<"), "no UTC hour");
  const utc = lineChart(chart(), "UTC");
  assert(utc.includes(">11:00<") && !utc.includes(">15:00<"), utc);
  assert(berlin.includes("Jan 15, 12:00"), "the table's row header");
});

test("the hidden table has a column per series and a row per x value", () => {
  const html = lineChart(chart(), "UTC");
  assert(html.includes("<caption>Temperatures &amp; more</caption>"), html);
  assert(html.includes('<th scope="col">Temperature</th><th scope="col">Feels like</th>'), html);
  assertEq(html.split('<th scope="row">').length - 1, 4);
  assert(html.includes("<td>4.6</td><td>1.9</td>"), html);
});

test("a series without a color takes the next chartSeries token; dashed and points are drawn", () => {
  const html = lineChart(chart(), "UTC");
  assert(html.includes("stroke: var(--k-color-chart-temperature)"), html);
  assert(html.includes("stroke: var(--k-color-chart-series2)"), "the second series' default");
  assertEq(html.split("stroke-dasharray").length - 1, 1, "only the dashed series");
  assertEq(html.split("<circle").length - 1, 4, "the markers of the series that shows points");
  assert(html.includes("var(--k-color-chart-grid)") && html.includes("var(--k-color-chart-axis)"), "grid and axis tokens");
});

test("the legend can be hidden, and the height is the svg's", () => {
  assert(lineChart(chart(), "UTC").includes("<ul"), "a legend by default");
  assert(!lineChart(chart({ legend: false }), "UTC").includes("<ul"), "no legend");
  assert(lineChart(chart({ height: "240px" }), "UTC").includes('viewBox="0 0 640 240"'));
  assert(lineChart(chart(), "UTC").includes('viewBox="0 0 640 320"'), "320px by default");
});

test("numbers on the x axis, a fixed y range and axis titles", () => {
  const series = [{ name: "n", points: [{ x: 1, y: 5 }, { x: 2, y: 7 }], color: null, dashed: false, showsPoints: false }];
  const html = lineChart({ series, description: "d", yRange: closedRange(0, 10), yAxisTitle: "°C", xAxisTitle: "Day" }, "UTC");
  assert(html.includes(">10<") && html.includes(">0<"), "the range's ends are ticks");
  assert(html.includes(">°C</text>") && html.includes(">Day</text>"), html);
  assert(html.includes('<th scope="col">Day</th>'), "the x title heads the table");
});

test("an empty chart still renders its description and an empty table", () => {
  const html = lineChart({ series: [], description: "Nothing yet" }, "UTC");
  assert(html.includes('aria-label="Nothing yet"') && html.includes("<tbody></tbody>"), html);
  assert(!html.includes("NaN") && !html.includes("Infinity"), html);
});

test("the same chart has the same key; another chart another key", () => {
  assertEq(chartKey(lineChart(chart(), "UTC")), chartKey(lineChart(chart(), "UTC")));
  assert(chartKey(lineChart(chart(), "UTC")) !== chartKey(lineChart(chart(), "Europe/Berlin")));
});

/** The browser props of a `LineChart` over the signal `series`, overridden by `over`. */
const props = (series, over = {}) => ({ series: () => series.get(), description: () => "d", timeZone: () => "UTC", ...over });

test("in the browser the chart renders and follows its data", () => {
  const series = signal([temperature]);
  const el = LineChart(props(series), {});
  assertEq(el.getAttribute("class"), "k-chart");
  assert(el.innerHTML.includes("<svg"), el.innerHTML);
  const before = el.getAttribute("data-k-chart");
  series.set([temperature, feelsLike]);
  flush();
  assert(el.getAttribute("data-k-chart") !== before, "rendered again");
  assert(el.innerHTML.includes("Feels like"), el.innerHTML);
});

test("hydration keeps the server's chart when it is the same chart", () => {
  const html = lineChart({ series: [temperature], description: "d" }, "UTC");
  const app = document.createElement("div");
  app.innerHTML = `<div class="k-chart" data-k-chart="${chartKey(html)}">${html}</div>`;
  const server = app.firstChild;
  const svg = server.firstChild;
  const series = signal([temperature]);
  hydrate(app, () => LineChart(props(series), {}), {});
  assertEq(app.firstChild, server, "the server's element");
  assertEq(server.firstChild, svg, "and its contents");
});

test("hydration renders the chart again when the server drew another one", () => {
  const html = lineChart({ series: [temperature], description: "d" }, "Europe/Berlin");
  const app = document.createElement("div");
  app.innerHTML = `<div class="k-chart" data-k-chart="${chartKey(html)}">${html}</div>`;
  const series = signal([temperature]);
  hydrate(app, () => LineChart(props(series), {}), {});
  assert(app.firstChild.innerHTML.includes(">11:00<"), "redrawn in the browser's zone");
});
