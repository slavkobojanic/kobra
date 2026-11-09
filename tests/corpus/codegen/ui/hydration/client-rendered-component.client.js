// file: shared.[hash].js
import * as $k from "./runtime.[hash].js";

const TodoList$t0 = $k.template("<h2> </h2>");
const TodoList$t1 = $k.template("<p> </p>");

export async function count($c, title) {
  return $k.graphemeCount(title);
}

function TodoList$view(props, ctx) {
  if (props.rows === undefined) {
    const $d1 = 3;
    props.rows = () => $d1;
  }
  const shown = $k.signal(0);
  const $loads = $k.createLoads([{ name: "total", after: [], run: async (v, $c) => {
    return $k.live($c, await $k.guard($c, count($c, props.title())));
  }, inputs: () => [props.title()] }], null);
  const $m6 = $k.markers();
  $k.branch($m6.start, $m6.end, () => [0, 1, 1, 2][$loads.state()], [() => {
    const $root4 = TodoList$t1();
    const $e5 = $root4.firstChild;
    $k.renderEffect(() => $k.text($e5, `Loading ${props.title()} (${String(props.rows())} rows)…`));
    return $root4;
  }, () => {
    const $root2 = TodoList$t0();
    const $e3 = $root2.firstChild;
    $k.renderEffect(() => $k.text($e3, `${props.title()}: ${String(shown.get())} of ${String($loads.values().total)}`));
    return $root2;
  }, () => null]);
  return $m6.node;
}

export function TodoList(props, ctx) {
  if (props.rows === undefined) {
    const $d1 = 3;
    props.rows = () => $d1;
  }
  const $m9 = $k.markers();
  const $r10 = $k.clientReady();
  $k.branch($m9.start, $m9.end, () => $r10() ? 1 : 0, [() => {
    const $root7 = TodoList$t1();
    const $e8 = $root7.firstChild;
    $k.renderEffect(() => $k.text($e8, `Loading ${props.title()} (${String(props.rows())} rows)…`));
    return $root7;
  }, () => TodoList$view(props, { ...ctx, data: null })]);
  return $m9.node;
}
