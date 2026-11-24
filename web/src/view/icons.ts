// The few icons beside words, built as `<path>`s through `dom.ts::svg`, never parsed; colour is `currentColor`.

import { svg } from "./dom";

const FRAME = {
  viewBox: "0 0 24 24",
  width: "22",
  height: "22",
  fill: "none",
  stroke: "currentColor",
  "stroke-width": "1.7",
  "stroke-linecap": "round",
  "stroke-linejoin": "round",
  "aria-hidden": "true",
  focusable: "false",
};

function icon(...paths: string[]): SVGSVGElement {
  return svg(
    "svg",
    FRAME,
    paths.map((d) => svg("path", { d })),
  );
}

export const chevronIcon = (): SVGSVGElement => icon("M8.5 5.5l7 6.5-7 6.5");

// The act icons, each beside its word: a tick alone is a guess about what "done" means.

/** A tick: complete. */
export const checkIcon = (): SVGSVGElement => icon("M4.5 12.5l5 5 10-11");

/** A cross: cancel — dropped, not finished. */
export const crossIcon = (): SVGSVGElement => icon("M6 6l12 12", "M18 6L6 18");

/** An arrow curving back: reopen. */
export const undoIcon = (): SVGSVGElement => icon("M4 9h9.5a5.5 5.5 0 0 1 0 11H8", "M8 4.5L3.5 9 8 13.5");

/** A tick inside a shield: confirm — a trusted hand on provisional content. */
export const confirmIcon = (): SVGSVGElement =>
  icon("M12 3.2l7 2.6v6.1c0 4.2-2.9 7.2-7 8.9-4.1-1.7-7-4.7-7-8.9V5.8z", "M8.6 11.8l2.5 2.5 4.4-4.8");

export const plusIcon = (): SVGSVGElement => icon("M12 5v14", "M5 12h14");

export const clipIcon = (): SVGSVGElement =>
  icon("M19.5 11.5l-7.6 7.6a4.8 4.8 0 0 1-6.8-6.8l8.3-8.3a3.2 3.2 0 0 1 4.5 4.5l-8.3 8.3a1.6 1.6 0 0 1-2.3-2.3l7.6-7.6");

export const docIcon = (): SVGSVGElement => icon("M7 3.5h7l4 4v13H7z", "M14 3.5v4h4");

export const lockIcon = (): SVGSVGElement => icon("M6.5 11h11v9h-11z", "M9 11V8a3 3 0 0 1 6 0v3");

export const sendIcon = (): SVGSVGElement => icon("M12 19V5.5", "M6.5 11L12 5.5l5.5 5.5");


export const searchIcon = (): SVGSVGElement => icon("M10.5 4.5a6 6 0 1 1 0 12 6 6 0 0 1 0-12z", "M15 15l4.5 4.5");
