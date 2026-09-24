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
import type {
  ContrastPair,
  ContrastVerdict,
  ImageScene,
  Medium,
  Roles,
  Scene,
  WebScene,
} from "../state";

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
 *
 * A stylesheet's: four that build the page, three that carry type, and
 * four that are only ever seen as a fill with something written on it.
 *
 * The last group arrived with the second and third scenes. A gradient needs a
 * far end, and a dense interface has states — saved, expiring, failed — that a
 * palette either distinguishes or does not. They are also the roles most often
 * left empty, which is why the preview draws them in the app's own grey until
 * they are filled: a status chip in a colour nobody chose should look like a
 * slot, not like a decision.
 */
const WEB_ROLES: [string, string][] = [
  ["background", "Behind everything"],
  ["surface", "Cards and panels"],
  ["border", "Outlines and rules"],
  ["text", "Body copy"],
  ["muted", "Secondary copy"],
  ["primary", "Buttons and links"],
  ["onPrimary", "Text on primary"],
  ["secondary", "The gradient's far end"],
  ["success", "Done, saved, healthy"],
  ["warning", "Needs attention"],
  ["danger", "Failed, destructive"],
];

/**
 * The picture's roles are the three values of a notan plus two focal colours,
 * because that is what both scenes are built from: every face of the pallet
 * and every plane of the portrait is light, mid or dark, and each picture has
 * one thing meant to be looked at first and one small mark. Naming them
 * "light/mid/dark" rather than "shape/second form" says what the job is — a
 * palette that reads here is one whose values separate, which is the thing a
 * strip of swatches hides best.
 *
 * The ids are shared by both scenes and only the hints change, so switching
 * subject keeps every assignment.
 */
const POSTER_ROLES: [string, string][] = [
  ["light", "Paper and top faces"],
  ["mid", "The faces between"],
  ["dark", "Shade, shadow, title band"],
  ["accent", "The disc behind"],
  ["highlight", "The one small mark"],
];

const COMPOSITION_ROLES: [string, string][] = [
  ["light", "Lit planes of the skin"],
  ["mid", "Shadow planes, hair, jacket"],
  ["dark", "The mass, and the deepest shade"],
  ["accent", "The backdrop"],
  ["highlight", "Earring and zip"],
];

/**
 * The roles a medium's scene fills.
 *
 * The three web scenes share one list — a surface is a surface whether it is a
 * card on a page or a panel in an app — while the two pictures each name their
 * five differently, since a poster has a title band and a portrait has a jaw.
 */
export function rolesFor(medium: Medium, scene: Scene): [string, string][] {
  if (medium !== "image") return WEB_ROLES;
  return scene === "composition" ? COMPOSITION_ROLES : POSTER_ROLES;
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
  scene: Scene,
  roles: Roles,
  colours: string[],
): number {
  return rolesFor(medium, scene).filter(([id]) => roleColour(roles, colours, id))
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
  scene: Scene,
  roles: Roles,
  colours: string[],
): ContrastPair[] {
  if (!medium) return [];
  const at = (role: string) => roleColour(roles, colours, role);
  const pairs: ContrastPair[] = [];
  const add = (id: string, fore?: string, back?: string, graphic = false) => {
    if (fore && back) pairs.push({ id, fore, back, graphic });
  };

  if (medium === "image" && scene === "composition") {
    // Nothing here is lettering, so nothing is held to the text bar: a
    // painting has no body copy, and judging a shadow plane at 4.5:1 would
    // fail palettes that are doing exactly what was asked of them.
    //
    // The silhouette against the ground is the first read, and the two value
    // steps inside the figure are the second: light on mid is the face, mid
    // on dark is the figure against its own mass. A palette that loses either
    // gives a portrait that reads as a flat shape.
    add("figure", at("dark"), at("accent"), true);
    add("planes", at("mid"), at("dark"), true);
    add("skin", at("light"), at("mid"), true);
    // The earring and the zip are four pixels wide on a dark ground, which is
    // the hardest thing in either picture to keep visible.
    add("mark", at("highlight"), at("dark"), true);
    return pairs;
  }

  if (medium === "image") {
    // The poster's two lines of lettering are the only things here held to the
    // text bar. The "2.0" is lettering too, but at 78px bold it is large text,
    // which WCAG judges at 3:1 — the same bar `graphic` already applies.
    add("title", at("light"), at("dark"));
    add("subtitle", at("mid"), at("dark"));
    add("version", at("highlight"), at("dark"), true);
    // The disc has to be found on the paper, and the pallet has to be read on
    // the disc — at both ends of its value range, since a palette can easily
    // separate one and swallow the other.
    add("disc", at("accent"), at("light"), true);
    add("lit", at("light"), at("accent"), true);
    add("shade", at("dark"), at("accent"), true);
    // And the turn of each board: the mid value against the light one is what
    // makes the boxes read as solid rather than as flat outline.
    add("facet", at("mid"), at("light"), true);
    return pairs;
  }

  if (scene === "app") {
    // An interface is read on its panels, not on the page behind them.
    add("card", at("text"), at("surface"));
    add("quiet", at("muted"), at("surface"));
    add("cta", at("onPrimary"), at("primary"));
    // The message under a field that has been filled in wrong. Body text, and
    // the one status colour that is regularly asked to be a sentence.
    add("error", at("danger"), at("surface"));
    add("rule", at("border"), at("surface"), true);
    // A ring around the focused field and a chip beside a row: both are marks
    // that only have to be findable, so 3:1 is the bar.
    add("ring", at("primary"), at("surface"), true);
    add("flag", at("success"), at("surface"), true);
    return pairs;
  }

  if (scene === "components") {
    // The sheet exists to ask one question five times: does the ink that was
    // chosen for the primary button also work on every other fill? A palette
    // usually has one light colour doing all of it, and the status colours are
    // where that assumption quietly breaks.
    add("cta", at("onPrimary"), at("primary"));
    add("far", at("onPrimary"), at("secondary"));
    add("onSuccess", at("onPrimary"), at("success"));
    add("onWarning", at("onPrimary"), at("warning"));
    add("onDanger", at("onPrimary"), at("danger"));
    add("card", at("text"), at("surface"));
    add("rule", at("border"), at("surface"), true);
    return pairs;
  }

  // The page. Long-form reading happens on the background here, not on a
  // panel, which is the pairing a card-only preview never judged.
  add("body", at("text"), at("background"));
  add("aside", at("muted"), at("background"));
  add("card", at("text"), at("surface"));
  add("cta", at("onPrimary"), at("primary"));
  // The hero runs `primary` into `secondary`, and a label that clears the
  // first can fail the second — the usual way a gradient goes wrong.
  add("far", at("onPrimary"), at("secondary"));
  // The card's outline and its ground: a shape, so 3:1 rather than 4.5:1.
  add("rule", at("border"), at("background"), true);
  // The card against the page. Also a shape — it is the card's edge that has
  // to be findable, not any text.
  add("panel", at("surface"), at("background"), true);
  return pairs;
}

/** Human wording for each pairing, so the verdict row says what it judged. */
const PAIR_LABELS: Record<string, string> = {
  body: "Body on the page",
  aside: "Secondary on the page",
  card: "Body on a panel",
  quiet: "Secondary on a panel",
  cta: "Label on the button",
  far: "Label on the gradient's far end",
  rule: "Border on its ground",
  panel: "Card on the page",
  error: "Error text on a panel",
  ring: "Focus ring on a panel",
  flag: "Status chip on a panel",
  onSuccess: "Label on success",
  onWarning: "Label on warning",
  onDanger: "Label on danger",
  title: "Title on the band",
  subtitle: "Subtitle on the band",
  version: "The 2.0 on the band",
  disc: "Disc on the paper",
  lit: "Top faces on the disc",
  shade: "Shaded faces on the disc",
  facet: "Side face on top face",
  figure: "Figure on the backdrop",
  planes: "Shadow planes on the mass",
  skin: "Lit skin on shadowed skin",
  mark: "Earring and zip on the mass",
};

/**
 * What an unassigned web role is drawn with.
 *
 * The page's own furniture borrows the app's furniture — the window's page
 * colour for the page, its panel for a panel — so an undressed preview reads
 * as Pallet rather than as a palette nobody chose. The four newest roles have
 * no counterpart in this window at all, and two of them fall back to plain
 * grey on purpose: a status chip in a colour nobody picked should look like an
 * empty slot, not like a decision about what "healthy" means.
 */
const WEB_STAND_INS: Record<string, string> = {
  background: "var(--bg)",
  surface: "var(--panel)",
  border: "var(--line)",
  text: "var(--ink)",
  muted: "var(--mute)",
  primary: "var(--accent)",
  onPrimary: "var(--accentInk)",
  secondary: "var(--accentDeep)",
  success: "var(--mute)",
  warning: "var(--mute)",
  danger: "var(--danger)",
};

/** The eleven role colours, as custom properties for a scene's root to carry. */
function webVars(roles: Roles, colours: string[]): string {
  return Object.keys(WEB_STAND_INS)
    .map(
      (role) =>
        `--w-${role}:${roleColour(roles, colours, role) ?? WEB_STAND_INS[role]}`,
    )
    .join(";");
}

/** A role, as it is written in the mock's styles. */
const w = (role: string) => `var(--w-${role})`;

/** The mock's type sizes. Small, but never smaller than it has to be. */
const HEAD = `600 17px/1.08 ${SANS}`;
const LEAD = `400 8.5px/1.45 ${SANS}`;
const BODY = `400 7.5px/1.35 ${SANS}`;
const TINY = `400 7px/1 ${MONO}`;

/**
 * A button in the mock.
 *
 * `invert` and `outline` are the two that sit on the gradient, where the page's
 * ordinary buttons would disappear: one takes the surface as a fill, the other
 * is drawn in the same ink as the words beside it.
 */
type ButtonKind =
  | "primary"
  | "secondary"
  | "invert"
  | "outline"
  | "ghost"
  | "disabled";

function wButton(label: string, kind: ButtonKind): HTMLElement {
  const skin: Record<ButtonKind, string> = {
    primary: `background:${w("primary")};color:${w("onPrimary")}`,
    secondary: `background:${w("secondary")};color:${w("onPrimary")}`,
    invert: `background:${w("surface")};color:${w("primary")}`,
    outline: `border:1px solid ${w("onPrimary")};color:${w("onPrimary")}`,
    ghost: `border:1px solid ${w("border")};color:${w("text")}`,
    disabled: `border:1px solid ${w("border")};color:${w("muted")};opacity:.5`,
  };
  return el("span", {
    style:
      `padding:5px 9px;border-radius:5px;font:500 8px/1 ${SANS};` +
      `white-space:nowrap;flex:none;${skin[kind]}`,
    text: label,
  });
}

/**
 * A status chip: the fill is the status role, the lettering is `onPrimary`.
 *
 * One ink across every fill rather than an `onSuccess`, `onWarning` and
 * `onDanger` to go with them — which is what nearly every palette does in
 * practice, and the assumption the components sheet exists to check.
 */
function wChip(label: string, role: string): HTMLElement {
  return el("span", {
    style:
      `padding:3px 6px;border-radius:999px;font:600 6.5px/1 ${MONO};` +
      `letter-spacing:.08em;flex:none;background:${w(role)};color:${w("onPrimary")}`,
    text: label,
  });
}

/** A text field, at rest, focused, or refused. */
function wField(text: string, state: "rest" | "focus" | "invalid"): HTMLElement {
  const edge =
    state === "focus"
      ? w("primary")
      : state === "invalid"
        ? w("danger")
        : w("border");
  return el("div", {
    style:
      "height:19px;display:flex;align-items:center;padding:0 7px;" +
      `border-radius:5px;background:${w("surface")};border:1px solid ${edge};` +
      `font:${BODY};color:${state === "rest" ? w("muted") : w("text")};` +
      // The ring is the primary at full strength rather than a wash of it:
      // there is no role for a translucent tint, and a focus ring that cannot
      // be seen is the accessibility bug this preview is meant to catch.
      (state === "focus" ? `box-shadow:0 0 0 2px ${w("primary")}` : ""),
    text,
  });
}

/** The window every web scene is drawn inside. */
function webFrame(vars: string, children: HTMLElement[]): HTMLElement {
  return el(
    "div",
    {
      style:
        `${vars};border-radius:var(--rad2);overflow:hidden;` +
        `background:${w("background")};` +
        "box-shadow:var(--lift),inset 0 0 0 1px rgba(var(--swatchInk),.09)",
    },
    children,
  );
}

/**
 * The page: a site, with a gradient across its hero.
 *
 * Long-form reading happens on the background here rather than on a panel,
 * which is the pairing the old single card could not judge — and the hero runs
 * `primary` into `secondary`, so the button label on it is asked to survive
 * both ends of the ramp at once.
 *
 * There was a browser's title bar over this, dots and a URL and all. It was
 * the one part of the mock drawn in the palette that the palette would never
 * actually be responsible for — a browser paints its own chrome — so it was
 * taking a fifth of the height to show colours doing somebody else's job.
 */
function pageScene(vars: string): HTMLElement {
  const nav = el(
    "div",
    { style: "display:flex;align-items:center;gap:11px;padding:10px 12px" },
    [
      el("span", {
        style: `font:600 9.5px/1 ${SANS};color:${w("text")};letter-spacing:.02em`,
        text: "Pallet",
      }),
      ...["Work", "Studio", "Journal"].map((t) =>
        el("span", {
          style: `font:400 8px/1 ${SANS};color:${w("muted")}`,
          text: t,
        }),
      ),
      el("div", { style: "flex:1" }),
      wButton("Get started", "primary"),
    ],
  );

  const hero = el(
    "div",
    {
      style:
        "margin:0 12px;padding:15px 14px;border-radius:9px;display:flex;" +
        "flex-direction:column;gap:7px;" +
        `background:linear-gradient(118deg, ${w("primary")}, ${w("secondary")})`,
    },
    [
      el("span", {
        style: `font:${HEAD};color:${w("onPrimary")}`,
        text: "Colour, decided.",
      }),
      el("span", {
        style: `font:${LEAD};color:${w("onPrimary")};opacity:.82;max-width:232px`,
        text: "Pull a palette off anything on screen, name it, and ship the stylesheet.",
      }),
      el("div", { style: "display:flex;gap:6px;padding-top:2px" }, [
        wButton("Download", "invert"),
        wButton("Read the docs", "outline"),
      ]),
    ],
  );

  const card = (title: string, line: string, role: string) =>
    el(
      "div",
      {
        style:
          `flex:1;min-width:0;padding:8px;border-radius:7px;display:flex;` +
          `flex-direction:column;gap:5px;background:${w("surface")};` +
          `border:1px solid ${w("border")}`,
      },
      [
        el("div", {
          style: `width:14px;height:14px;border-radius:4px;background:${w(role)}`,
        }),
        el("span", {
          style: `font:600 8px/1 ${SANS};color:${w("text")}`,
          text: title,
        }),
        el("span", { style: `font:${BODY};color:${w("muted")}`, text: line }),
      ],
    );

  const cards = el("div", { style: "display:flex;gap:7px;padding:12px" }, [
    card("Pick", "From any window on screen.", "primary"),
    card("Name", "Tokens that survive export.", "secondary"),
    card("Ship", "CSS, SCSS, Tailwind, ASE.", "success"),
  ]);

  const footer = el(
    "div",
    {
      style:
        "display:flex;align-items:center;gap:9px;padding:9px 12px;" +
        `border-top:1px solid ${w("border")}`,
    },
    [
      el("span", {
        style: `font:${TINY};color:${w("muted")}`,
        text: "© Pallet",
      }),
      el("div", { style: "flex:1" }),
      ...["Terms", "Privacy"].map((t) =>
        el("span", {
          style: `font:400 7px/1 ${SANS};color:${w("muted")}`,
          text: t,
        }),
      ),
    ],
  );

  return webFrame(vars, [nav, hero, cards, footer]);
}

/**
 * The app: the same palette asked to run an interface.
 *
 * Where the page has areas, this has states — a selected row, a focused field,
 * a refused one, a disabled button, three chips saying three different things.
 * A palette can carry a page beautifully and still leave "saved" and "failed"
 * looking identical, which is the failure this scene is here to show.
 */
function appScene(vars: string): HTMLElement {
  const item = (label: string, on: boolean) =>
    el("div", {
      style:
        `padding:5px 7px;border-radius:5px;font:${on ? "600" : "400"} 7.5px/1 ${SANS};` +
        (on
          ? `background:${w("primary")};color:${w("onPrimary")}`
          : `color:${w("muted")}`),
      text: label,
    });

  const sidebar = el(
    "div",
    {
      style:
        "width:96px;flex:none;padding:9px 8px;display:flex;" +
        `flex-direction:column;gap:3px;background:${w("surface")};` +
        `border-right:1px solid ${w("border")}`,
    },
    [
      el(
        "div",
        { style: "display:flex;align-items:center;gap:5px;padding:0 3px 8px" },
        [
          el("div", {
            style: `width:11px;height:11px;border-radius:3px;background:${w("primary")}`,
          }),
          el("span", {
            style: `font:600 8.5px/1 ${SANS};color:${w("text")}`,
            text: "Pallet",
          }),
        ],
      ),
      item("Overview", true),
      item("Library", false),
      item("Team", false),
      item("Settings", false),
    ],
  );

  const topbar = el(
    "div",
    {
      style:
        "display:flex;align-items:center;gap:7px;padding:7px 9px;" +
        `border-bottom:1px solid ${w("border")}`,
    },
    [
      el("div", {
        style:
          `flex:1;height:17px;border-radius:999px;background:${w("surface")};` +
          `border:1px solid ${w("border")};display:flex;align-items:center;` +
          `padding:0 8px;font:${BODY};color:${w("muted")}`,
        text: "Search the library…",
      }),
      el("div", {
        style: `width:16px;height:16px;border-radius:999px;flex:none;background:${w("secondary")}`,
      }),
    ],
  );

  const row = (name: string, chip: HTMLElement, alt: boolean) =>
    el(
      "div",
      {
        style:
          "display:flex;align-items:center;gap:8px;padding:6px 8px;" +
          (alt ? `background:${w("surface")}` : ""),
      },
      [
        el("span", {
          style: `flex:1;min-width:0;font:${BODY};color:${w("text")}`,
          text: name,
        }),
        chip,
      ],
    );

  const table = el(
    "div",
    {
      style: `border:1px solid ${w("border")};border-radius:6px;overflow:hidden`,
    },
    [
      row("Autumn campaign", wChip("LIVE", "success"), false),
      row("Studio rebrand", wChip("DRAFT", "warning"), true),
      row("Retired marks", wChip("FAILED", "danger"), false),
    ],
  );

  const form = el(
    "div",
    { style: "display:flex;flex-direction:column;gap:5px" },
    [
      el("span", {
        style: `font:500 6.5px/1 ${MONO};letter-spacing:.14em;color:${w("muted")}`,
        text: "PALETTE NAME",
      }),
      wField("Autumn campaign", "focus"),
      wField("Autumn campaign", "invalid"),
      el("span", {
        style: `font:400 7px/1.35 ${SANS};color:${w("danger")}`,
        text: "That name is already in the library.",
      }),
    ],
  );

  const actions = el(
    "div",
    { style: "display:flex;gap:6px;align-items:center" },
    [
      wButton("Save", "primary"),
      wButton("Discard", "ghost"),
      el("div", { style: "flex:1" }),
      wButton("Export", "disabled"),
    ],
  );

  const main = el(
    "div",
    { style: "flex:1;min-width:0;display:flex;flex-direction:column" },
    [
      topbar,
      el(
        "div",
        {
          style:
            "padding:9px;display:flex;flex-direction:column;gap:9px;min-width:0",
        },
        [form, table, actions],
      ),
    ],
  );

  return webFrame(vars, [el("div", { style: "display:flex" }, [sidebar, main])]);
}

/**
 * The sheet: every control, in every state, with nothing around it.
 *
 * Not a scene, and not pretending to be one. It is the page a design system
 * ends up drawing, and it answers the question neither of the others can ask
 * cleanly: a palette usually has exactly one light colour doing the lettering
 * on every fill it owns, and this is where that assumption meets the primary,
 * the gradient's far end, and all three status colours at once.
 */
function componentsScene(vars: string): HTMLElement {
  const label = (t: string) =>
    el("span", {
      style: `font:400 6.5px/1 ${MONO};letter-spacing:.14em;color:${w("muted")}`,
      text: t,
    });

  const band = (name: string, items: HTMLElement[]) =>
    el("div", { style: "display:flex;flex-direction:column;gap:7px" }, [
      label(name),
      el(
        "div",
        { style: "display:flex;gap:6px;align-items:center;flex-wrap:wrap" },
        items,
      ),
    ]);

  const toggle = (on: boolean) =>
    el(
      "div",
      {
        style:
          "width:25px;height:14px;border-radius:999px;padding:2px;display:flex;" +
          "flex:none;align-items:center;" +
          (on
            ? `background:${w("primary")};justify-content:flex-end`
            : `background:${w("border")}`),
      },
      [
        el("div", {
          style:
            "width:10px;height:10px;border-radius:999px;" +
            `background:${w(on ? "onPrimary" : "surface")}`,
        }),
      ],
    );

  const progress = el(
    "div",
    {
      style:
        `flex:1;min-width:0;height:6px;border-radius:999px;overflow:hidden;` +
        `background:${w("border")}`,
    },
    [
      el("div", {
        style:
          "width:62%;height:100%;" +
          `background:linear-gradient(90deg, ${w("primary")}, ${w("secondary")})`,
      }),
    ],
  );

  // The gradient with the same two letters at each end: the verdict below
  // reports this pairing as a number, and this is the number being looked at.
  const ramp = el(
    "div",
    {
      style:
        "height:28px;border-radius:6px;display:flex;align-items:center;" +
        "justify-content:space-between;padding:0 9px;" +
        `background:linear-gradient(90deg, ${w("primary")}, ${w("secondary")})`,
    },
    [
      el("span", {
        style: `font:600 9px/1 ${SANS};color:${w("onPrimary")}`,
        text: "Aa",
      }),
      el("span", {
        style: `font:600 9px/1 ${SANS};color:${w("onPrimary")}`,
        text: "Aa",
      }),
    ],
  );

  const sheet = el(
    "div",
    {
      style:
        "margin:12px;padding:11px;border-radius:9px;display:flex;" +
        `flex-direction:column;gap:11px;background:${w("surface")};` +
        `border:1px solid ${w("border")}`,
    },
    [
      el("div", { style: "display:flex;flex-direction:column;gap:3px" }, [
        el("span", {
          style: `font:600 9.5px/1 ${SANS};color:${w("text")}`,
          text: "Components",
        }),
        el("span", {
          style: `font:${BODY};color:${w("muted")}`,
          text: "Every fill, lettered in the one ink.",
        }),
      ]),
      band("BUTTONS", [
        wButton("Primary", "primary"),
        wButton("Secondary", "secondary"),
        wButton("Ghost", "ghost"),
        wButton("Disabled", "disabled"),
      ]),
      band("FIELDS", [
        el("div", { style: "display:flex;gap:6px;width:100%" }, [
          el("div", { style: "flex:1;min-width:0" }, [wField("Rest", "rest")]),
          el("div", { style: "flex:1;min-width:0" }, [
            wField("Focused", "focus"),
          ]),
          el("div", { style: "flex:1;min-width:0" }, [
            wField("Refused", "invalid"),
          ]),
        ]),
      ]),
      band("STATUS", [
        wChip("SUCCESS", "success"),
        wChip("WARNING", "warning"),
        wChip("DANGER", "danger"),
        wChip("BETA", "secondary"),
      ]),
      band("CONTROLS", [toggle(true), toggle(false), progress]),
      band("GRADIENT", [el("div", { style: "width:100%" }, [ramp])]),
    ],
  );

  return webFrame(vars, [sheet]);
}

/** The picture a stylesheet's colours end up in, and the switch between them. */
function webScenes(
  scene: WebScene,
  roles: Roles,
  colours: string[],
  actions: PreviewActions,
): HTMLElement {
  const vars = webVars(roles, colours);
  const drawing =
    scene === "app"
      ? appScene(vars)
      : scene === "components"
        ? componentsScene(vars)
        : pageScene(vars);

  return el(
    "div",
    { style: "display:flex;flex-direction:column;gap:8px;align-items:center" },
    [
      el("div", { style: "width:100%" }, [drawing]),
      sceneToggle("web", scene, actions),
    ],
  );
}

/**
 * The stack of pallets, as [value, polygon] pairs in painting order.
 *
 * Geometry rather than drawing code: every face is a flat quad in an
 * isometric projection, so there is nothing to compute at render time and
 * nothing that can drift from the poster this was traced off. The order is the
 * z-order — a board's far edge, its end cap, then its top — and cannot be
 * sorted by value without the stack coming apart.
 */
const PALLET_FACES: ["light" | "mid" | "dark", string][] = [
  ["mid", "233.1,368.7 472.1,506.7 472.1,498.6 233.1,360.6"],
  ["dark", "501.0,490.0 472.1,506.7 472.1,498.6 501.0,481.9"],
  ["light", "262.0,343.9 501.0,481.9 472.1,498.6 233.1,360.6"],
  ["mid", "148.0,417.8 387.0,555.8 387.0,547.7 148.0,409.7"],
  ["dark", "415.9,539.2 387.0,555.8 387.0,547.7 415.9,531.1"],
  ["light", "176.8,393.1 415.9,531.1 387.0,547.7 148.0,409.7"],
  ["mid", "62.8,467.0 301.8,605.0 301.8,596.9 62.8,458.9"],
  ["dark", "330.7,588.3 301.8,605.0 301.8,596.9 330.7,580.2"],
  ["light", "91.7,442.2 330.7,580.2 301.8,596.9 62.8,458.9"],
  ["mid", "233.1,360.6 262.0,377.3 262.0,348.6 233.1,331.9"],
  ["dark", "290.9,360.6 262.0,377.3 262.0,348.6 290.9,331.9"],
  ["light", "262.0,315.2 290.9,331.9 262.0,348.6 233.1,331.9"],
  ["mid", "148.0,409.7 176.8,426.4 176.8,397.7 148.0,381.0"],
  ["dark", "205.7,409.7 176.8,426.4 176.8,397.7 205.7,381.0"],
  ["light", "176.8,364.4 205.7,381.0 176.8,397.7 148.0,381.0"],
  ["mid", "338.2,421.2 367.1,437.9 367.1,409.2 338.2,392.5"],
  ["dark", "396.0,421.2 367.1,437.9 367.1,409.2 396.0,392.5"],
  ["light", "367.1,375.9 396.0,392.5 367.1,409.2 338.2,392.5"],
  ["mid", "62.8,458.9 91.7,475.6 91.7,446.9 62.8,430.2"],
  ["dark", "120.6,458.9 91.7,475.6 91.7,446.9 120.6,430.2"],
  ["light", "91.7,413.5 120.6,430.2 91.7,446.9 62.8,430.2"],
  ["mid", "253.0,470.4 281.9,487.1 281.9,458.4 253.0,441.7"],
  ["dark", "310.8,470.4 281.9,487.1 281.9,458.4 310.8,441.7"],
  ["light", "281.9,425.0 310.8,441.7 281.9,458.4 253.0,441.7"],
  ["mid", "443.3,481.9 472.1,498.6 472.1,469.9 443.3,453.2"],
  ["dark", "501.0,481.9 472.1,498.6 472.1,469.9 501.0,453.2"],
  ["light", "472.1,436.5 501.0,453.2 472.1,469.9 443.3,453.2"],
  ["mid", "167.9,519.6 196.8,536.2 196.8,507.5 167.9,490.9"],
  ["dark", "225.6,519.6 196.8,536.2 196.8,507.5 225.6,490.9"],
  ["light", "196.8,474.2 225.6,490.9 196.8,507.5 167.9,490.9"],
  ["mid", "358.1,531.1 387.0,547.7 387.0,519.0 358.1,502.4"],
  ["dark", "415.9,531.1 387.0,547.7 387.0,519.0 415.9,502.4"],
  ["light", "387.0,485.7 415.9,502.4 387.0,519.0 358.1,502.4"],
  ["mid", "273.0,580.2 301.8,596.9 301.8,568.2 273.0,551.5"],
  ["dark", "330.7,580.2 301.8,596.9 301.8,568.2 330.7,551.5"],
  ["light", "301.8,534.9 330.7,551.5 301.8,568.2 273.0,551.5"],
  ["mid", "62.8,430.2 91.7,446.9 91.7,438.8 62.8,422.1"],
  ["dark", "290.9,331.9 91.7,446.9 91.7,438.8 290.9,323.8"],
  ["light", "262.0,307.1 290.9,323.8 91.7,438.8 62.8,422.1"],
  ["mid", "167.9,490.9 196.8,507.5 196.8,499.4 167.9,482.8"],
  ["dark", "396.0,392.5 196.8,507.5 196.8,499.4 396.0,384.4"],
  ["light", "367.1,367.8 396.0,384.4 196.8,499.4 167.9,482.8"],
  ["mid", "273.0,551.5 301.8,568.2 301.8,560.1 273.0,543.4"],
  ["dark", "501.0,453.2 301.8,568.2 301.8,560.1 501.0,445.1"],
  ["light", "472.1,428.4 501.0,445.1 301.8,560.1 273.0,543.4"],
  ["mid", "233.1,323.8 472.1,461.8 472.1,453.7 233.1,315.7"],
  ["dark", "501.0,445.1 472.1,461.8 472.1,453.7 501.0,437.0"],
  ["light", "262.0,299.0 501.0,437.0 472.1,453.7 233.1,315.7"],
  ["mid", "190.5,348.4 429.6,486.4 429.6,478.3 190.5,340.3"],
  ["dark", "458.4,469.7 429.6,486.4 429.6,478.3 458.4,461.6"],
  ["light", "219.4,323.6 458.4,461.6 429.6,478.3 190.5,340.3"],
  ["mid", "148.0,372.9 387.0,510.9 387.0,502.8 148.0,364.8"],
  ["dark", "415.9,494.3 387.0,510.9 387.0,502.8 415.9,486.2"],
  ["light", "176.8,348.2 415.9,486.2 387.0,502.8 148.0,364.8"],
  ["mid", "105.4,397.5 344.4,535.5 344.4,527.4 105.4,389.4"],
  ["dark", "373.3,518.8 344.4,535.5 344.4,527.4 373.3,510.8"],
  ["light", "134.3,372.8 373.3,510.8 344.4,527.4 105.4,389.4"],
  ["mid", "62.8,422.1 301.8,560.1 301.8,552.0 62.8,414.0"],
  ["dark", "330.7,543.4 301.8,560.1 301.8,552.0 330.7,535.3"],
  ["light", "91.7,397.3 330.7,535.3 301.8,552.0 62.8,414.0"],
];

/**
 * The portrait, as flat notan layers in painting order.
 *
 * Traced off the source painting rather than drawn by rule, which is why it is
 * five long paths and not a figure built from shapes: the value boundaries of
 * a real head are not describable as circles, and a diagram of a face would
 * test a palette against a diagram. Every layer is one of the three values, so
 * the figure carries the whole range and the backdrop is the only colour that
 * is not part of it.
 */
const PORTRAIT_LAYERS: ["light" | "mid" | "dark", string][] = [
  [
    "dark",
    "M356,118 331,124 213,208 209,224 197,236 202,247 169,330 158,386 151,403 150,550 133,575 121,602 117,666 102,717 104,733 101,739 101,762 112,769 120,802 129,812 142,846 142,863 120,879 114,893 117,920 142,957 132,989 113,1001 114,1007 128,1012 139,1022 124,1064 125,1085 135,1113 85,1236 82,1253 91,1260 104,1251 113,1250 131,1269 156,1274 216,1306 250,1319 320,1337 408,1331 442,1333 456,1329 476,1333 527,1333 568,1327 583,1338 591,1327 625,1325 652,1311 707,1310 745,1302 781,1287 794,1276 803,1260 812,1223 797,1191 771,1172 766,1164 743,1090 734,1030 702,1021 645,959 639,916 667,877 670,830 662,810 666,797 657,789 658,779 631,737 645,729 661,737 667,729 656,717 623,699 628,693 670,686 688,662 692,641 689,615 698,598 695,579 703,565 698,549 700,520 713,490 702,456 715,444 721,422 727,422 730,429 737,428 754,388 750,344 753,319 741,269 724,226 690,184 667,167 657,164 625,167 574,147 534,119 514,112 488,117 410,112Z M701,432 706,438 698,450 693,446 693,438Z M703,403 710,416 710,425 705,429 698,420 698,407Z",
  ],
  [
    "mid",
    "M742,1090 718,1139 734,1189 764,1213 779,1245 776,1265 792,1276 812,1223 797,1191 768,1168Z M661,791 648,831 634,840 634,899 606,946 616,988 672,1045 701,1051 698,1028 683,1043 666,1029 658,974 645,959 639,916 669,869Z M610,752 598,762 628,813 637,756 623,760Z M633,745 626,713 588,719Z M734,403 719,417 733,430 741,420Z M656,166 641,194 691,248 713,311 728,304 742,344 750,346 751,307 724,226 685,179Z M629,168 617,164 587,186 629,187Z M376,131 371,119 333,123 311,141Z M519,114 514,150 529,160 533,119Z M407,114 411,133 450,138 482,134 487,120Z",
  ],
  [
    "mid",
    "M629,216 605,221 590,232 526,232 497,247 476,294 467,386 455,419 443,435 437,434 453,391 455,333 450,328 421,362 394,415 376,431 378,454 375,465 381,481 374,501 364,496 337,462 298,441 281,442 262,450 244,470 236,494 234,522 251,548 288,574 318,587 356,597 357,617 353,628 360,641 365,641 364,603 380,596 389,585 396,587 416,641 416,647 404,653 404,675 411,684 412,703 420,727 436,756 440,793 478,847 486,853 550,832 565,823 571,802 564,793 549,783 526,782 521,774 545,722 566,694 574,689 586,689 619,698 626,693 662,689 677,680 689,653 688,616 698,597 694,581 703,565 698,551 700,519 712,483 705,465 692,448 693,437 688,438 682,433 687,427 698,426 698,393 695,390 698,379 688,372 673,371 670,364 676,358 694,355 696,350 687,316 683,274 674,258Z M684,569 678,576 664,574 679,566Z M587,420 586,427 563,436 538,425 541,420Z M614,357 610,364 515,365 528,353 578,346 604,349Z",
  ],
  [
    "dark",
    "M407,612 416,647 404,655 404,675 411,684 412,703 420,727 431,745 436,744 436,736 429,719 430,698 437,690 478,701 513,720 527,716 542,698 539,687 487,668 442,645 424,629 412,611Z M362,593 357,597 356,620 359,622 364,603 369,599Z M686,564 678,559 666,566 640,569 636,576 661,577 666,570 683,568Z M265,448 257,457 266,463 253,477 241,503 243,522 256,543 256,550 261,554 269,552 300,571 329,576 343,572 345,566 341,562 307,556 292,543 279,497 280,483 285,477 308,479 310,483 299,512 298,529 302,540 307,543 316,537 329,548 357,547 361,544 360,536 348,530 340,521 338,509 327,503 331,489 328,474 295,458 280,457 271,461 267,456 281,445 277,443Z M443,437 432,436 428,454 433,453Z M384,424 376,431 378,454 375,465 381,481 374,501 364,496 340,465 335,466 351,488 352,495 364,504 365,515 382,522 391,520 396,510 387,460 387,426Z M517,367 512,362 492,372 487,380 498,380Z M613,348 610,350 614,357 612,365 620,368 625,364 625,358Z M450,328 439,338 447,347 447,365 450,367 455,350 455,333Z",
  ],
  [
    "light",
    "M486,750 479,760 477,774 489,799 487,817 502,827 489,846 501,848 555,829 567,811 559,790 545,783 531,790 516,790 512,778 501,767 497,753 491,748Z M537,728 524,749 526,759 539,735Z M648,583 650,590 660,597 698,597 697,587 691,579 654,579Z M526,244 505,264 489,292 479,373 483,375 548,343 615,345 626,352 627,362 616,372 620,380 617,401 604,421 596,418 603,397 582,367 541,364 515,372 518,385 512,401 519,413 552,420 581,416 594,419 596,424 553,446 534,444 500,426 494,431 488,449 534,505 568,581 584,591 610,588 619,597 621,605 610,627 609,654 612,660 623,667 647,672 669,665 678,676 689,653 690,611 665,609 642,593 622,587 618,579 665,547 697,548 703,508 698,507 688,517 669,514 625,527 610,515 607,499 624,471 640,458 651,465 652,471 642,491 630,502 632,510 651,494 678,489 696,489 707,497 711,493 712,483 705,465 693,450 692,443 680,433 683,427 697,424 697,379 686,372 671,373 666,368 675,351 691,344 685,313 685,279 656,246 624,230 598,238Z",
  ],
];

/**
 * What an unassigned role is drawn with, per scene.
 *
 * A picture is never blank, and the stand-ins read as Pallet's own colours
 * rather than as colours the user chose.
 *
 * The poster takes ordinary tokens and lets them invert with the theme: on
 * Studio the paper goes dark and the band goes pale, which is a poster
 * somebody might have printed. The portrait cannot afford that — a face with
 * white hair and a black cheek reads as a negative — so it takes the ladder
 * `tokens.css` writes out per theme, where the light value stays the lightest
 * in both. That file has the long version of the argument.
 */
const STAND_INS: Record<ImageScene, Record<string, string>> = {
  poster: {
    light: "var(--panel)",
    mid: "var(--accentSoft)",
    dark: "var(--ink)",
    accent: "var(--accentFaint)",
    highlight: "var(--accent)",
  },
  composition: {
    light: "var(--portraitLight)",
    mid: "var(--portraitMid)",
    dark: "var(--portraitDark)",
    accent: "var(--portraitGround)",
    highlight: "var(--accent)",
  },
};

/** The five role colours, as custom properties for a scene's root to carry. */
function sceneVars(scene: ImageScene, roles: Roles, colours: string[]): string {
  const stand = STAND_INS[scene];
  return Object.keys(stand)
    .map(
      (role) => `--p-${role}:${roleColour(roles, colours, role) ?? stand[role]}`,
    )
    .join(";");
}

/**
 * `var()` goes in a style attribute rather than in `fill`: presentation
 * attributes do not take custom properties in every engine, a style
 * declaration does everywhere.
 */
const fill = (role: string) => `fill:var(--p-${role})`;

/** How tall either picture is drawn, in a window only 464px wide. */
const SCENE_HEIGHT = 330;

/** The root a scene hangs off: its own proportions, sized by height. */
function sceneRoot(viewBox: string, vars: string): SVGElement {
  return svg("svg", {
    viewBox,
    // Sized by height and left to work out its own width, so each picture
    // keeps its proportions and sits at a readable size rather than filling
    // the window top to bottom.
    style:
      `${vars};display:block;height:${SCENE_HEIGHT}px;width:auto;` +
      "max-width:100%;border-radius:4px",
    preserveAspectRatio: "xMidYMid meet",
  });
}

/**
 * The designer's picture: a poster of the thing itself.
 *
 * A mid-century notan — three values, a focal disc, one small mark — rather
 * than a mockup of a photograph, because a contact sheet is not a screen and
 * drawing a fake photograph would invite the palette to be judged on how well
 * it matched a picture nobody is making. What a poster does test is the thing
 * a swatch strip cannot show: whether the values separate when they are shapes
 * meeting edge to edge, at the top of the range and at the bottom, with
 * lettering sitting on one of them.
 *
 * Drawn as SVG for the same reason the gear and the cross are: it has to be
 * the same on every machine, and a face-down glyph font is not.
 */
function posterScene(vars: string): SVGElement {
  const poster = sceneRoot("0 0 600 900", vars);

  const paper = svg("rect", {
    width: "600",
    height: "900",
    style: fill("light"),
  });

  const disc = svg("circle", {
    cx: "300",
    cy: "400",
    r: "228",
    style: fill("accent"),
  });

  // One cast shadow for the whole stack, thrown off the bottom board.
  const shadow = svg("polygon", {
    points:
      "62.8,467.0 262.0,352.0 348.0,375.2 587.1,513.2 387.9,628.2 301.8,605.0",
    style: fill("dark"),
  });

  const faces = PALLET_FACES.map(([role, points]) =>
    svg("polygon", { points, style: fill(role) }),
  );

  const band = svg("rect", {
    y: "712",
    width: "600",
    height: "188",
    style: fill("dark"),
  });

  const title = svg("text", {
    x: "48",
    y: "808",
    "font-size": "78",
    "font-weight": "700",
    "letter-spacing": "2",
    style: `font-family:${SANS}`,
  });
  const word = svg("tspan", { style: fill("light") });
  word.textContent = "PALLET";
  const version = svg("tspan", { dx: "18", style: fill("highlight") });
  version.textContent = "2.0";
  title.append(word, version);

  const subtitle = svg("text", {
    x: "50",
    y: "850",
    "font-size": "19",
    "letter-spacing": "1",
    style: `${fill("mid")};font-family:${SANS}`,
  });
  subtitle.textContent = "a pallet composition study";

  poster.append(paper, disc, shadow, ...faces, band, title, subtitle);
  return poster;
}

/**
 * The painter's picture: a head and shoulders against a flat ground.
 *
 * A figure rather than a second arrangement of shapes, because the question a
 * painter brings is not whether two areas separate but whether a palette can
 * still describe a form once it is spread over one: the same three values have
 * to turn a cheek, hold a jaw, and keep the hair from falling into the coat.
 * A palette can pass the poster and fail here — that is the whole reason both
 * exist.
 */
function compositionScene(vars: string): SVGElement {
  const portrait = sceneRoot("0 0 862 1392", vars);

  const backdrop = svg("rect", {
    width: "862",
    height: "1392",
    style: fill("accent"),
  });

  const layers = PORTRAIT_LAYERS.map(([role, d]) =>
    // Even-odd because each layer is one path with its own holes cut in it —
    // the eye sockets, the gap under the jaw — rather than a stack of shapes.
    svg("path", { d, "fill-rule": "evenodd", style: fill(role) }),
  );

  // The zip: a drawn line rather than a filled sliver, so the highlight is
  // tested at the width it is actually used at.
  const zip = svg("polyline", {
    points: "486,860 516,935 560,1045 606,1175 636,1300",
    "stroke-width": "4",
    "stroke-linejoin": "round",
    style: "fill:none;stroke:var(--p-highlight)",
  });

  const earring = svg("circle", {
    cx: "365.0",
    cy: "666.9",
    r: "38.2",
    style: fill("highlight"),
  });

  portrait.append(backdrop, ...layers, zip, earring);
  return portrait;
}

/**
 * The picture, and the switch between the two of them.
 *
 * The switch sits under the drawing rather than up beside the medium toggle:
 * it changes what is being looked at, not what the palette is for, and two
 * pill rows in one heading would read as one four-way choice.
 */
function imageScene(
  scene: ImageScene,
  roles: Roles,
  colours: string[],
  actions: PreviewActions,
): HTMLElement {
  const vars = sceneVars(scene, roles, colours);
  const drawing =
    scene === "composition" ? compositionScene(vars) : posterScene(vars);

  const frame = el(
    "div",
    {
      style:
        "display:flex;justify-content:center;padding:14px;" +
        "border-radius:var(--rad2);background:var(--panel);overflow:hidden;" +
        "box-shadow:var(--lift),inset 0 0 0 1px rgba(var(--swatchInk),.09)",
    },
    [drawing],
  );

  return el(
    "div",
    { style: "display:flex;flex-direction:column;gap:8px;align-items:center" },
    [frame, sceneToggle("image", scene, actions)],
  );
}

export interface PreviewActions {
  /**
   * Choose what the palette is for.
   *
   * Keeps the roles, which is safe because the two media share no role names:
   * a stylesheet's `surface` and a picture's `mid` cannot be mistaken for one
   * another, so the assignments made under one simply sit dormant while the
   * other is on screen.
   */
  onMedium: (medium: Medium | null) => void;
  /** Choose which subject this medium draws. Keeps the roles. */
  onScene: (scene: Scene) => void;
  /** Unassign every role the current medium has. Leaves the other's alone. */
  onClearRoles: () => void;
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

/**
 * A row of small pills, one of them current.
 *
 * Shared by the two switches the preview has, so the one that changes the
 * medium and the one that changes the subject are visibly the same kind of
 * control rather than two inventions.
 */
function pills<T extends string>(
  options: [T, string][],
  current: T,
  pick: (id: T) => void,
): HTMLElement {
  return el(
    "div",
    { style: "display:flex;gap:3px;flex:none" },
    options.map(([id, label]) => {
      const on = id === current;
      return el("span", {
        class: on ? undefined : "clickable",
        style:
          `padding:4px 8px;border-radius:6px;font:${on ? "600" : "400"} 9px/1 ${MONO};` +
          "letter-spacing:.06em;" +
          (on
            ? "background:var(--accent);color:var(--accentInk);"
            : "color:var(--mute);cursor:pointer"),
        text: label.toUpperCase(),
        onClick: on ? undefined : () => pick(id),
      });
    }),
  );
}

/** The switch back to the other medium, shown beside the PREVIEW heading. */
export function mediumToggle(
  medium: Medium,
  actions: PreviewActions,
): HTMLElement {
  return pills(
    MEDIA.map(([id, label]) => [id, label] as [Medium, string]),
    medium,
    (id) => actions.onMedium(id),
  );
}

/**
 * What each medium can be looking at, in the order the switch offers them.
 *
 * The first of each is the default, and both are the plainest of their set: a
 * poster before a portrait, a page before an interface.
 */
const IMAGE_SCENES: [ImageScene, string][] = [
  ["poster", "Poster"],
  ["composition", "Composition"],
];

const WEB_SCENES: [WebScene, string][] = [
  ["page", "Page"],
  ["app", "App"],
  ["components", "Components"],
];

/** The switch between a medium's subjects, shown under whichever is drawn. */
function sceneToggle(
  medium: Medium,
  scene: Scene,
  actions: PreviewActions,
): HTMLElement {
  const options: [Scene, string][] =
    medium === "image" ? [...IMAGE_SCENES] : [...WEB_SCENES];
  return pills<Scene>(options, scene, (id) => actions.onScene(id));
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
  scene: Scene,
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
    scene:
      medium === "image"
        ? imageScene(scene as ImageScene, roles, colours, actions)
        : webScenes(scene as WebScene, roles, colours, actions),
    roles: group(
      rolesFor(medium, scene).map(([id, hint]) =>
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
