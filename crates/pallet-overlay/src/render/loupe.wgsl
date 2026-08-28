// The frozen desktop and the loupe over it.
//
// Colour fidelity is the whole point of this shader, so it does no colour
// maths on the source at all. The frozen frame is uploaded as a non-sRGB
// `Unorm` texture and written to a non-sRGB target, so captured bytes reach
// the display unchanged. Sampling is `textureLoad` rather than a filtered
// sampler: interpolation would invent colours that are not on screen, and the
// pixel the user aims at must be the pixel they get.

struct Uniforms {
    cursor: vec2<f32>,
    zoom: f32,
    radius: f32,
    sample: f32,
    grid: f32,
    _pad: vec2<f32>,
    picked: vec4<f32>,
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

fn source_at(p: vec2<i32>) -> vec4<f32> {
    let size = vec2<i32>(textureDimensions(frozen));
    let c = clamp(p, vec2<i32>(0, 0), size - vec2<i32>(1, 1));
    return textureLoad(frozen, c, 0);
}

@fragment
fn fs(@builtin(position) frag: vec4<f32>) -> @location(0) vec4<f32> {
    let pos = frag.xy;

    // The frozen desktop, 1:1 and untouched.
    var out = source_at(vec2<i32>(floor(pos)));

    let offset = pos - u.cursor;
    let dist = length(offset);

    // Outside the loupe the screen is exactly as captured.
    if (dist > u.radius) {
        return out;
    }

    // Which source pixel this part of the loupe magnifies.
    let source = u.cursor + offset / u.zoom;
    let texel = vec2<i32>(floor(source));
    out = source_at(texel);

    // Pixel grid, drawn on the boundary between magnified texels. Only once
    // the zoom is high enough that lines separate pixels rather than swallow
    // them.
    if (u.grid > 0.5 && u.zoom >= 8.0) {
        let within = fract(source);
        let edge = 1.0 / u.zoom;
        if (within.x < edge || within.y < edge) {
            // Translucent dark reads on light and dark content alike without
            // hiding the colour underneath.
            out = mix(out, vec4<f32>(0.0, 0.0, 0.0, 1.0), 0.25);
        }
    }

    // Outline the pixels a commit would take.
    //
    // Drawn in *screen* space, on the border of the magnified region rather
    // than on the texels themselves. Painting the texels would cover the very
    // pixel the user is aiming at - which at sample size 1 means obscuring
    // the only pixel that matters.
    let half_sample = floor(u.sample * 0.5);
    let edge = (half_sample + 0.5) * u.zoom;
    let reach = max(abs(offset.x), abs(offset.y));
    if (reach <= edge && reach > edge - 1.5) {
        let luma = dot(out.rgb, vec3<f32>(0.2126, 0.7152, 0.0722));
        let ink = select(1.0, 0.0, luma > 0.5);
        out = vec4<f32>(vec3<f32>(ink), 1.0);
    }

    // The rim: a ring of the picked colour between thin contrast lines, so it
    // reads against any background.
    let rim = u.radius - dist;
    if (rim < 10.0) {
        let luma = dot(u.picked.rgb, vec3<f32>(0.2126, 0.7152, 0.0722));
        let ink = select(1.0, 0.0, luma > 0.5);
        if (rim < 1.5 || rim > 8.5) {
            out = vec4<f32>(vec3<f32>(ink), 1.0);
        } else {
            out = vec4<f32>(u.picked.rgb, 1.0);
        }
    }

    return out;
}
