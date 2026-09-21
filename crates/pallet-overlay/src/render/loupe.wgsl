// The frozen desktop, the crosshair and the loupe.
//
// Colour fidelity is the whole point of this shader, so it does no colour
// maths on the source at all. The frozen frame is uploaded as a non-sRGB
// `Unorm` texture and written to a non-sRGB target, so captured bytes reach
// the display unchanged. Sampling is `textureLoad` rather than a filtered
// sampler: interpolation would invent colours that are not on screen, and the
// pixel the user aims at must be the pixel they get.
//
// Every measurement below is from `Prototype/Pallet Pick.dc.html`, in design
// pixels, scaled by `u.scale`. The loupe is a 176px circle showing 11 cells of
// 16px at 16x.

struct Uniforms {
    cursor: vec2<f32>,
    zoom: f32,
    radius: f32,
    sample: f32,
    grid: f32,
    scale: f32,
    vignette: f32,
    // How the display this frame came from is rotated, matching
    // `Monitor::transform`: 0 upright, 1/2/3 for 90/180/270 degrees, 4
    // mirrored. See `source_at`.
    transform: u32,
};

@group(0) @binding(0) var frozen: texture_2d<f32>;
@group(0) @binding(1) var<uniform> u: Uniforms;

@vertex
fn vs(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    // One oversized triangle covering the viewport; cheaper than a quad and
    // with no seam along a diagonal.
    let x = f32(i32(index) / 2) * 4.0 - 1.0;
    let y = f32(i32(index) & 1) * 4.0 - 1.0;
    return vec4<f32>(x, -y, 0.0, 1.0);
}

// Read a source pixel by its position in the *displayed* image.
//
// The texture holds the framebuffer exactly as the capture backend handed it
// over, which on a rotated display is the panel's own grid: 3840x2160 behind a
// portrait 2160x3840 desktop. Turning it upright used to be a CPU pass over
// every pixel before upload, which cost ten times what an upright monitor did
// for the same pixel count — a rotation reads down a column while writing
// along a row, so it misses cache on nearly every one of them. Doing it here
// instead costs a handful of integer operations per fragment on hardware built
// for exactly that, and lets the upload be a straight memcpy for every
// display rather than only unrotated ones.
//
// This mirrors `Monitor::displayed_to_buffer`, which is what the picker reads
// the committed colour through; the two disagreeing would show one pixel and
// pick another, so `a_rotated_display_is_uploaded_upright` checks this against
// that function rather than against a hand-copied rotation.
fn source_at(p: vec2<i32>) -> vec4<f32> {
    let size = vec2<i32>(textureDimensions(frozen));
    let bw = size.x - 1;
    let bh = size.y - 1;

    var b = p;
    switch u.transform {
        case 1u: { b = vec2<i32>(p.y, bh - p.x); }
        case 2u: { b = vec2<i32>(bw - p.x, bh - p.y); }
        case 3u: { b = vec2<i32>(bw - p.y, p.x); }
        // Mirroring is rare enough that Pallet handles the horizontal flip and
        // treats any rotation on top of it as unrotated, matching the Rust
        // side rather than guessing at a combination neither can test.
        case 4u: { b = vec2<i32>(bw - p.x, p.y); }
        default: {}
    }

    let c = clamp(b, vec2<i32>(0, 0), size - vec2<i32>(1, 1));
    return textureLoad(frozen, c, 0);
}

fn over(base: vec3<f32>, ink: vec3<f32>, alpha: f32) -> vec3<f32> {
    return mix(base, ink, clamp(alpha, 0.0, 1.0));
}

@fragment
fn fs(@builtin(position) frag: vec4<f32>) -> @location(0) vec4<f32> {
    let pos = frag.xy;
    let s = u.scale;
    var out = source_at(vec2<i32>(floor(pos))).rgb;

    let offset = pos - u.cursor;
    let dist = length(offset);
    let ring_outer = u.radius + 4.0 * s;

    // --- Crosshair -------------------------------------------------------
    //
    // `background:rgba(255,255,255,.5); mix-blend-mode:difference`. Difference
    // against white is `1 - backdrop`, and mixing that with the backdrop at
    // half strength lands on a flat 50% grey whatever is underneath — that
    // cancellation is the design's, not an approximation of it.
    if (floor(pos.x) == floor(u.cursor.x) || floor(pos.y) == floor(u.cursor.y)) {
        out = mix(out, vec3<f32>(1.0) - out, 0.5);
    }

    // --- Drop shadow -----------------------------------------------------
    //
    // `0 18px 44px -12px rgba(0,0,0,.65)`: the circle again, pushed down 18px,
    // shrunk by 12px, and softened over the blur radius.
    if (dist > ring_outer) {
        let to_shadow = length(pos - (u.cursor + vec2<f32>(0.0, 18.0 * s)));
        let edge = ring_outer - 12.0 * s;
        let soften = 22.0 * s;
        let cover = 1.0 - smoothstep(edge - soften, edge + soften, to_shadow);
        return vec4<f32>(over(out, vec3<f32>(0.0), cover * 0.65), 1.0);
    }

    // --- Ring ------------------------------------------------------------
    //
    // `0 0 0 1px rgba(0,0,0,.35), 0 0 0 4px rgba(255,255,255,.92)`. The first
    // shadow paints over the second, so the innermost pixel of the ring is
    // black at 35% on top of white at 92%.
    if (dist > u.radius) {
        let fade = 1.0 - smoothstep(ring_outer - 1.0, ring_outer, dist);
        out = over(out, vec3<f32>(1.0), 0.92 * fade);
        if (dist <= u.radius + 1.0 * s) {
            out = over(out, vec3<f32>(0.0), 0.35);
        }
        return vec4<f32>(out, 1.0);
    }

    // --- Magnified pixels -------------------------------------------------
    let source = u.cursor + offset / u.zoom;
    var loupe = source_at(vec2<i32>(floor(source)));

    // Pixel grid on the cell boundaries: `rgba(0,0,0,.14)` hairlines every
    // 16px. Only once the zoom is high enough that the lines separate pixels
    // rather than swallow them.
    if (u.grid > 0.5 && u.zoom >= 8.0) {
        let within = fract(source);
        let edge = s / u.zoom;
        if (within.x < edge || within.y < edge) {
            loupe = vec4<f32>(over(loupe.rgb, vec3<f32>(0.0), 0.14), 1.0);
        }
    }

    // The cell that a commit would take: `0 0 0 1.5px #fff, 0 0 0 3px
    // rgba(0,0,0,.55)` around a 16px box — one cell at 16x, and the whole
    // averaged square when Shift widens the sample.
    //
    // Drawn in screen space on the *border* of that region. Painting the cell
    // itself would cover the very pixel the user is aiming at.
    let box = (floor(u.sample * 0.5) + 0.5) * u.zoom;
    let reach = max(abs(offset.x), abs(offset.y));
    if (reach > box && reach <= box + 3.0 * s) {
        if (reach <= box + 1.5 * s) {
            loupe = vec4<f32>(1.0, 1.0, 1.0, 1.0);
        } else {
            loupe = vec4<f32>(over(loupe.rgb, vec3<f32>(0.0), 0.55), 1.0);
        }
    }

    // `inset 0 0 26px rgba(0,0,0,.28)`, which seats the circle over the page.
    //
    // This is the one part of the design that tints pixels the user might
    // pick, so its strength is a uniform rather than a constant: the pixel
    // under the crosshair is far enough from the rim to be untouched, but a
    // colour read off the loupe's edge is darkened, and anyone who wants the
    // loupe strictly faithful edge to edge can turn it off.
    //
    // A CSS inset shadow is the shape's inverse, blurred: it reaches half
    // strength *at* the edge and fades to nothing about half the blur radius
    // inside. Ramping from full strength at the edge instead would make the
    // rim twice as dark as the design.
    let inward = u.radius - dist;
    let softness = 13.0 * s;
    loupe = vec4<f32>(
        over(loupe.rgb, vec3<f32>(0.0), u.vignette * (1.0 - smoothstep(-softness, softness, inward))),
        1.0
    );

    // Antialias where the magnified content meets the ring.
    return vec4<f32>(mix(loupe.rgb, out, smoothstep(u.radius - 1.0, u.radius, dist)), 1.0);
}
