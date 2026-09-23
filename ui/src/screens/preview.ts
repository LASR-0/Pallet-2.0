/**
 * What a palette looks like doing a job.
 *
 * A palette read as a row of swatches tells you very little about whether it
 * works: colours that look handsome side by side routinely turn out to be
 * unreadable one on top of the other, and a strip cannot show that. This draws
 * the palette doing real work and then says, in numbers, whether the pairings
 * it produced are usable.
 *
 * Which work depends on the medium. The formats Pallet writes fall into two
 * kinds and they fail in different ways: a stylesheet's colours become a
 * themed interface, where the question is whether text can be read on a panel;
 * a contact sheet's colours become a picture, where the question is whether
 * shapes read against each other and nothing is text at all. So the medium is
 * asked for first, and it decides the roles, the scene and the pairings alike.
 *
 * Roles rather than tokens do the assigning. A token is what a colour is
 * *called* when it is written out; a role is what it *does*. Conflating them
 * would mean a palette could only be previewed once it had been named — which
 * is backwards, since seeing it work is what tells you what to call it.
 *
 * Every contrast figure comes from the backend. `pallet_color::contrast` holds
 * both WCAG 2.1 and APCA, and a second implementation of either in TypeScript
 * is the drift this codebase keeps out; see `api.contrastPairs`.
 */

import { el, svg } from "../dom";
import type { ContrastPair, ContrastVerdict, Medium, Roles } from "../state";

const MONO = "var(--mono),monospace";
const SANS = "var(--font),sans-serif";

/** The media, as the chooser lists them: id, name, and what it covers. */
export const MEDIA: [Medium, string, string][] = [
  ["web", "Web", "CSS · SCSS · Tailwind · tokens"],
  ["image", "Image", "PNG contact sheet"],
];

/**
 * The jobs a colour can hold, per medium.
 *
 * Small fixed vocabularies rather than free text: these are the parts the
 * thing being made actually has, and a role that names nothing in the scene
 * would be a slot with nowhere to show its effect.
 */
const WEB_ROLES: [string, string][] = [
  ["background", "Behind everything"],
  ["surface", "Cards and panels"],
  ["border", "Outlines and rules"],
  ["text", "Body copy"],
  ["muted", "Secondary copy"],
  ["primary", "Buttons and links"],
  ["onPrimary", "Text on primary"],
];

const IMAGE_ROLES: [string, string][] = [
  ["background", "The sheet"],
  ["shape", "The largest form"],
  ["accent", "The second form"],
  ["highlight", "Small marks"],
  ["ink", "Lines and lettering"],
];

export function rolesFor(medium: Medium): [string, string][] {
  return medium === "image" ? IMAGE_ROLES : WEB_ROLES;
}

/**
 * A role's colour, or undefined if it has none that is still in the palette.
 *
 * Roles are held by hex, so a colour removed from the palette leaves its role
 * pointing at something no longer there. Checked here rather than cleaned up
 * on removal: one place that can be wrong beats several that must be kept
 * right, and the answer is the same either way.
 */
export function roleColour(
  roles: Roles,
  colours: string[],
  role: string,
): string | undefined {
  const hex = roles[role];
  return hex && colours.includes(hex) ? hex : undefined;
}

/** How many of this medium's roles are filled. */
export function filledRoles(
  medium: Medium,
  roles: Roles,
  colours: string[],
): number {
  return rolesFor(medium).filter(([id]) => roleColour(roles, colours, id))
    .length;
}

/**
 * The pairings the scene produces, for the backend to judge.
 *
 * Only pairs where both sides are assigned: a verdict on a colour the user has
 * not chosen yet would be a judgement on a default they never picked.
 *
 * `graphic` marks the pairs that are shapes rather than body text. WCAG asks
 * 4.5:1 of text and only 3:1 of anything else that has to be distinguishable,
 * so judging a card's outline — or a picture, where nothing is body text — by
 * the text bar would fail palettes that are perfectly usable.
 */
export function previewPairs(
  medium: Medium | null,
  roles: Roles,
  colours: string[],
): ContrastPair[] {
  if (!medium) return [];
  const at = (role: string) => roleColour(roles, colours, role);
  const pairs: ContrastPair[] = [];
  const add = (id: string, fore?: string, back?: string, graphic = false) => {
    if (fore && back) pairs.push({ id, fore, back, graphic });
  };

  if (medium === "image") {
    // Lettering on a poster is the one thing here held to the text bar; the
    // rest is form against form.
    add("ink", at("ink"), at("background"));
    add("shape", at("shape"), at("background"), true);
    add("accent", at("accent"), at("shape"), true);
    add("highlight", at("highlight"), at("shape"), true);
    return pairs;
  }

  add("text", at("text"), at("surface"));
  add("muted", at("muted"), at("surface"));
  add("onPrimary", at("onPrimary"), at("primary"));
  // The card's outline and its ground: a shape, so 3:1 rather than 4.5:1.
  add("border", at("border"), at("background"), true);
  // The card against the page. Also a shape — it is the card's edge that has
  // to be findable, not any text.
  add("surface", at("surface"), at("background"), true);
  return pairs;
}

/** Human wording for each pairing, so the verdict row says what it judged. */
const PAIR_LABELS: Record<string, string> = {
  text: "Body on surface",
  muted: "Secondary on surface",
  onPrimary: "Label on primary",
  border: "Border on background",
  surface: "Surface on background",
  ink: "Lettering on sheet",
  shape: "Form on sheet",
  accent: "Second form on the first",
  highlight: "Marks on the form",
};

/**
 * The scene a stylesheet's colours end up in: an ordinary card.
 *
 * Unassigned roles fall back to the app's own theme tokens, so the preview is
 * never blank and never pretends a role has been chosen — the stand-ins are
 * visibly Pallet's colours rather than the palette's.
 */
function webScene(roles: Roles, colours: string[]): HTMLElement {
  const at = (role: string) => roleColour(roles, colours, role);

  const background = at("background") ?? "var(--bg)";
  const surface = at("surface") ?? "var(--panel)";
  const border = at("border") ?? "var(--line)";
  const text = at("text") ?? "var(--ink)";
  const muted = at("muted") ?? "var(--mute)";
  const primary = at("primary") ?? "var(--accent)";
  // `--accentInk` rather than a literal white, because the role above it falls
  // back to `--accent` and these two are the pair: an unassigned scene should
  // show the window's own button, not a button half borrowed from it.
  const onPrimary = at("onPrimary") ?? "var(--accentInk)";

  return el(
    "div",
    {
      style:
        `padding:18px;border-radius:var(--rad2);background:${background};` +
        "box-shadow:var(--lift),inset 0 0 0 1px rgba(var(--swatchInk),.09)",
    },
    [
      el(
        "div",
        {
          style:
            `padding:16px;border-radius:10px;background:${surface};` +
            `border:1px solid ${border};display:flex;flex-direction:column;gap:10px`,
        },
        [
          el("div", { style: "display:flex;flex-direction:column;gap:5px" }, [
            el("span", {
              style: `font:600 15px/1.2 ${SANS};color:${text}`,
              text: "Autumn campaign",
            }),
            el("span", {
              style: `font:400 11px/1.45 ${SANS};color:${muted}`,
              text: "Six colours pulled from a photograph, ready for the site refresh.",
            }),
          ]),
          el("div", { style: "display:flex;gap:7px;align-items:center" }, [
            el("span", {
              style:
                `padding:8px 14px;border-radius:7px;background:${primary};` +
                `color:${onPrimary};font:500 11px/1 ${SANS}`,
              text: "Publish",
            }),
            el("span", {
              style:
                `padding:8px 14px;border-radius:7px;border:1px solid ${border};` +
                `color:${text};font:500 11px/1 ${SANS}`,
              text: "Discard",
            }),
            el("div", { style: "flex:1" }),
            el("span", {
              style: `font:400 10px/1 ${MONO};color:${primary}`,
              text: "Edit",
            }),
          ]),
        ],
      ),
    ],
  );
}

/**
 * The scene a picture's colours end up in.
 *
 * An abstract composition rather than a mockup of anything: a contact sheet is
 * not a screen, and drawing a fake photograph would invite the palette to be
 * judged on how well it matched a picture nobody is making. Overlapping forms,
 * one mark on top of another, and a line of lettering — enough for the colours
 * to be seen against each other in the several ways a picture does it.
 *
 * Drawn as SVG for the same reason the gear and the cross are: it has to be
 * the same on every machine, and a face-down glyph font is not.
 */
function imageScene(roles: Roles, colours: string[]): HTMLElement {
  const at = (role: string) => roleColour(roles, colours, role);

  const background = at("background") ?? "var(--panel)";
  const shape = at("shape") ?? "var(--accentSoft)";
  const accent = at("accent") ?? "var(--accentFaint)";
  const highlight = at("highlight") ?? "var(--accent)";
  const ink = at("ink") ?? "var(--ink)";

  const scene = svg("svg", {
    viewBox: "0 0 380 200",
    width: "100%",
    // The sheet keeps its proportions rather than stretching with the window,
    // which is what an exported picture does.
    preserveAspectRatio: "xMidYMid meet",
    style: "display:block;border-radius:10px;overflow:hidden",
  });

  const rect = (
    x: number,
    y: number,
    w: number,
    h: number,
    fill: string,
    rx = 0,
  ) =>
    svg("rect", {
      x: String(x),
      y: String(y),
      width: String(w),
      height: String(h),
      rx: String(rx),
      fill,
    });

  const circle = (cx: number, cy: number, r: number, fill: string) =>
    svg("circle", {
      cx: String(cx),
      cy: String(cy),
      r: String(r),
      fill,
    });

  scene.append(
    rect(0, 0, 380, 200, background),
    // The largest form: a band across the lower half, with a disc riding on
    // its edge so the two are seen both apart and overlapping.
    circle(268, 74, 46, shape),
    rect(0, 108, 380, 92, shape),
    // The second form, overlapping the first — the pairing the verdict below
    // reports as "second form on the first".
    svg("path", {
      d: "M0 140 C 90 104, 150 168, 230 136 C 300 108, 340 142, 380 126 L380 200 L0 200 Z",
      fill: accent,
    }),
    // Small marks, which is what a highlight has to survive being.
    circle(52, 56, 13, highlight),
    circle(92, 42, 6, highlight),
    rect(38, 160, 72, 6, highlight, 3),
    // Lettering.
    rect(38, 84, 104, 7, ink, 3.5),
    rect(38, 100, 64, 5, ink, 2.5),
  );

  return el(
    "div",
    {
      style:
        "border-radius:var(--rad2);overflow:hidden;" +
        "box-shadow:var(--lift),inset 0 0 0 1px rgba(var(--swatchInk),.09)",
    },
    [scene],
  );
}

export interface PreviewActions {
  /** Choose what the palette is for. Clears the roles; they are per-medium. */
  onMedium: (medium: Medium | null) => void;
  /** Open or close the colour chooser for a role. */
  onChooseRole: (role: string | null) => void;
  /** Give a role a colour, or clear it when the hex is null. */
  onAssignRole: (role: string, hex: string | null) => void;
}

/** The question asked before anything is drawn: what is this palette for? */
function mediumChooser(actions: PreviewActions): HTMLElement {
  return el(
    "div",
    { style: "display:grid;grid-template-columns:1fr 1fr;gap:8px" },
    MEDIA.map(([id, label, note]) =>
      el(
        "div",
        {
          class: "hv-card clickable",
          style:
            "display:flex;flex-direction:column;gap:4px;padding:14px 12px;" +
            "border-radius:var(--rad2);background:var(--panel);" +
            "border:1px solid var(--line);box-shadow:var(--lift)",
          onClick: () => actions.onMedium(id),
        },
        [
          el("span", { style: `font:600 12px/1 ${SANS}`, text: label }),
          el("span", {
            style: `font:400 9.5px/1.4 ${MONO};color:var(--mute)`,
            text: note,
          }),
        ],
      ),
    ),
  );
}

/** One role, its colour, and — when it is being chosen — the palette to pick from. */
function roleRow(
  id: string,
  hint: string,
  colours: string[],
  assigned: string | undefined,
  choosing: boolean,
  actions: PreviewActions,
): HTMLElement {
  const head = el(
    "div",
    {
      class: "hv-row clickable",
      style:
        "display:flex;align-items:center;gap:10px;padding:9px 11px;" +
        "background:var(--panel)",
      onClick: () => actions.onChooseRole(choosing ? null : id),
    },
    [
      el("div", {
        // A ring rather than a fill while unassigned, so an empty role reads
        // as a slot waiting for something instead of as a colour chosen.
        style:
          "width:18px;height:18px;flex:none;border-radius:5px;" +
          (assigned
            ? `background:${assigned};box-shadow:inset 0 0 0 1px rgba(var(--swatchInk),.12)`
            : "box-shadow:inset 0 0 0 1px var(--line)"),
      }),
      el("span", { style: `font:500 11px/1 ${SANS};flex:none`, text: id }),
      el("span", {
        style: `flex:1;min-width:0;font:400 10px/1 ${SANS};color:var(--mute)`,
        text: hint,
      }),
      el("span", {
        style: `font:400 9.5px/1 ${MONO};color:var(--mute);flex:none`,
        text: assigned ?? "—",
      }),
    ],
  );

  if (!choosing) return head;

  // The palette, to choose from. Shown under the row it belongs to rather than
  // in a menu: the colours are the point, and a menu of hex strings would make
  // the user read what they could otherwise just look at.
  const swatches = colours.map((hex) =>
    el("div", {
      class: "clickable",
      style:
        `width:26px;height:26px;border-radius:6px;background:${hex};` +
        (hex === assigned
          ? "box-shadow:inset 0 0 0 1px rgba(var(--swatchInk),.12),0 0 0 2px var(--accent)"
          : "box-shadow:inset 0 0 0 1px rgba(var(--swatchInk),.12)"),
      title: hex,
      onClick: () => actions.onAssignRole(id, hex),
    }),
  );

  const chooser = el(
    "div",
    {
      style:
        "display:flex;flex-wrap:wrap;gap:6px;align-items:center;" +
        "padding:10px 11px;background:var(--panelHi)",
    },
    [
      ...swatches,
      assigned
        ? el("span", {
            class: "clickable",
            style:
              `padding:5px 9px;border-radius:6px;font:400 9.5px/1 ${MONO};` +
              "color:var(--mute);border:1px solid var(--line)",
            text: "clear",
            onClick: () => actions.onAssignRole(id, null),
          })
        : null,
    ].filter((c): c is HTMLElement => c !== null),
  );

  return el("div", { style: "display:flex;flex-direction:column;gap:1px" }, [
    head,
    chooser,
  ]);
}

/** One judged pairing. */
function verdictRow(verdict: ContrastVerdict): HTMLElement {
  return el(
    "div",
    {
      style:
        "display:flex;align-items:center;gap:8px;padding:7px 11px;" +
        "background:var(--panel)",
    },
    [
      el("span", {
        style: `flex:1;min-width:0;font:400 10.5px/1 ${SANS}`,
        text: PAIR_LABELS[verdict.id] ?? verdict.id,
      }),
      el("span", {
        style: `font:400 9.5px/1 ${MONO};color:var(--mute);flex:none`,
        text: `${verdict.ratio.toFixed(2)}:1`,
      }),
      el("span", {
        // The verdict carries the colour, not the ratio beside it: the number
        // is evidence, the word is the answer.
        style:
          `font:500 9px/1 ${MONO};letter-spacing:.06em;flex:none;` +
          `color:${verdict.passes ? "var(--accent)" : "var(--danger)"}`,
        text: verdict.passes ? verdict.level.toUpperCase() : "FAIL",
      }),
    ],
  );
}

/** The grouped-row block used by Current and Settings. */
function group(children: HTMLElement[]): HTMLElement {
  return el(
    "div",
    {
      style:
        "display:flex;flex-direction:column;gap:1px;border-radius:var(--rad2);" +
        "overflow:hidden;background:var(--line);box-shadow:var(--lift)",
    },
    children,
  );
}

/** The switch back to the other medium, shown beside the PREVIEW heading. */
export function mediumToggle(
  medium: Medium,
  actions: PreviewActions,
): HTMLElement {
  return el(
    "div",
    { style: "display:flex;gap:3px;flex:none" },
    MEDIA.map(([id, label]) => {
      const on = id === medium;
      return el("span", {
        class: on ? undefined : "clickable",
        style:
          `padding:4px 8px;border-radius:6px;font:${on ? "600" : "400"} 9px/1 ${MONO};` +
          "letter-spacing:.06em;" +
          (on
            ? "background:var(--accent);color:var(--accentInk);"
            : "color:var(--mute);cursor:pointer"),
        text: label.toUpperCase(),
        onClick: on ? undefined : () => actions.onMedium(id),
      });
    }),
  );
}

/**
 * The preview: the scene, the roles that fill it, and what they add up to.
 *
 * Returns the pieces rather than one block, so the Build screen can space them
 * with its own section headings instead of this inventing a second rhythm.
 * `scene` is the chooser while the medium is unknown, since asking is the
 * first thing the preview has to do.
 */
export function renderPreview(
  medium: Medium | null,
  colours: string[],
  roles: Roles,
  verdicts: ContrastVerdict[] | null,
  choosingRole: string | null,
  actions: PreviewActions,
): {
  scene: HTMLElement;
  roles: HTMLElement | null;
  verdicts: HTMLElement | null;
} {
  if (!medium) {
    return { scene: mediumChooser(actions), roles: null, verdicts: null };
  }

  return {
    scene: medium === "image" ? imageScene(roles, colours) : webScene(roles, colours),
    roles: group(
      rolesFor(medium).map(([id, hint]) =>
        roleRow(
          id,
          hint,
          colours,
          roleColour(roles, colours, id),
          choosingRole === id,
          actions,
        ),
      ),
    ),
    verdicts:
      verdicts && verdicts.length > 0 ? group(verdicts.map(verdictRow)) : null,
  };
}
