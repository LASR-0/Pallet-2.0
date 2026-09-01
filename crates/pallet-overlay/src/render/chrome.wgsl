// Composites one CPU-rasterised HUD panel over the frozen screen.
//
// The panel is drawn at exactly 1:1 — `textureLoad`, not a sampler — because
// it was rasterised for this screen's pixel grid. Filtering it would soften
// text that is already antialiased and put a half-pixel of blur on every
// hairline border in the tray.

struct Quad {
    // x, y, width, height in target pixels.
    rect: vec4<f32>,
    // Target surface size in pixels, for the clip-space conversion.
    viewport: vec2<f32>,
    _pad: vec2<f32>,
};

@group(0) @binding(0) var panel: texture_2d<f32>;
@group(0) @binding(1) var<uniform> q: Quad;

struct Vertex {
    @builtin(position) clip: vec4<f32>,
    @location(0) local: vec2<f32>,
};

@vertex
fn vs(@builtin(vertex_index) index: u32) -> Vertex {
    // Two triangles, as a corner lookup: 0,1,2 then 2,1,3.
    let lookup = array<u32, 6>(0u, 1u, 2u, 2u, 1u, 3u);
    let corner = lookup[index];
    let unit = vec2<f32>(f32(corner & 1u), f32(corner >> 1u));

    let local = unit * q.rect.zw;
    let pixel = q.rect.xy + local;
    let ndc = pixel / q.viewport * 2.0 - vec2<f32>(1.0, 1.0);

    var out: Vertex;
    out.clip = vec4<f32>(ndc.x, -ndc.y, 0.0, 1.0);
    out.local = local;
    return out;
}

@fragment
fn fs(in: Vertex) -> @location(0) vec4<f32> {
    let size = vec2<i32>(textureDimensions(panel));
    let at = clamp(vec2<i32>(floor(in.local)), vec2<i32>(0, 0), size - vec2<i32>(1, 1));
    return textureLoad(panel, at, 0);
}
