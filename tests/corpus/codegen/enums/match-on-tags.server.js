// file: k:/app/corpus.js
import * as $k from "k:runtime";

export const Light = {
  off: $k.unitCase(0),
  on: (level) => ({ $: 1, level }),
};

export function brightness(light) {
  switch (light.$) {
    case 0:
      return 0;
    case 1: {
      const level = light.level;
      return level;
    }
    default:
      return $k.unreachable();
  }
}
