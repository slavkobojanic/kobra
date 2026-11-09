// file: k:/app/corpus.js
import * as $k from "k:runtime";
import * as $s from "k:runtime/server";

export async function count($c, title) {
  return $k.graphemeCount(title);
}

export function TodoList$ssr(props, ctx, out) {
  if (props.rows === undefined) {
    const $d1 = 3;
    props.rows = () => $d1;
  }
  out.raw($s.OPEN);
  out.raw("<p>");
  out.text(`Loading ${props.title()} (${String(props.rows())} rows)…`);
  out.raw("</p>");
  out.raw($s.CLOSE);
}
