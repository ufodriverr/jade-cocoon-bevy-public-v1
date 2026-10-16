// The picture of every screen laid into the window when the window does not show it one
// to one (`display.rs`). Drawn at the console's own pixels (`render = "authentic"`), it
// is enlarged block by block: inside a block its colour, across the seam to the next a
// blend one window pixel wide, so every block is as large as the next whatever the
// enlargement ("sharp bilinear"). `crt = "scanlines"` darkens the seam between two of the
// console's 240 lines; `crt = "crt"` does that a little less and adds a mask of the three
// colours, a slight glow and darker corners. Nothing is bent: a pixel of the window shows
// the picture where the mouse finds it. The camera composes in the stored colour values,
// so the result is turned back into them.

#import bevy_sprite::mesh2d_vertex_output::VertexOutput
#import bevy_render::color_operations::linear_to_srgb

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var picture: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var picture_sampler: sampler;
// xy: the picture's own pixels, zw: its size in the window.
@group(#{MATERIAL_BIND_GROUP}) @binding(2) var<uniform> size: vec4<f32>;
// x: 0 no filter, 1 scanlines, 2 crt; y: the console's lines down the picture.
@group(#{MATERIAL_BIND_GROUP}) @binding(3) var<uniform> look: vec4<f32>;

const PI: f32 = 3.14159265;
// How much darker the seam between two lines is than the middle of a line.
const SCANLINES: f32 = 0.5;
const CRT_SCANLINES: f32 = 0.4;
// How much of the other two colours a column of the mask takes away.
const MASK: f32 = 0.15;
// How much of the light round a pixel glows over it.
const GLOW: f32 = 0.3;
// How much darker the corners are.
const CORNERS: f32 = 0.35;

// The picture at a place, its pixels as blocks.
fn blocks(uv: vec2<f32>) -> vec3<f32> {
    let pixels = size.xy;
    let scale = max(size.zw / pixels, vec2(1.0));
    let texel = uv * pixels;
    let whole = floor(texel);
    let from_middle = texel - whole - 0.5;
    let inner = 0.5 - 0.5 / scale;
    let at = whole + 0.5 + (from_middle - clamp(from_middle, -inner, inner)) * scale;
    return textureSampleLevel(picture, picture_sampler, at / pixels, 0.0).rgb;
}

// The brightness of a window pixel across the console's lines: the upper part of a line
// bright, the seam to the next darker, so that at twice the console's size every other
// window line is the dark one. The wave is averaged over the window pixel's height, so
// lines of a few window pixels do not beat against them, and it fades out where a line
// has less than two window pixels.
fn scanline(y: f32, strength: f32) -> f32 {
    let per_line = size.w / look.y;
    let line = y * look.y;
    let height = PI * min(1.0 / per_line, 1.0);
    let averaged = max(sin(height) / height, 0.0);
    let wave = cos(2.0 * PI * (fract(line) - 0.25)) * averaged;
    let fade = clamp(per_line - 1.0, 0.0, 1.0);
    return 1.0 - strength * fade * (0.5 - 0.5 * wave);
}

// The mask of the three colours down the window's columns, as bright as no mask on the
// whole; none where a console pixel has less than three columns.
fn mask(column: f32) -> vec3<f32> {
    let per_line = size.w / look.y;
    let strength = MASK * clamp(per_line - 2.0, 0.0, 1.0);
    var weights = vec3(1.0 - strength);
    weights[u32(column) % 3u] = 1.0;
    return weights / (1.0 - strength * 2.0 / 3.0);
}

// The light round a place, one and a half console pixels away in eight ways.
fn glow(uv: vec2<f32>) -> vec3<f32> {
    let step = vec2(size.w / size.z, 1.0) / look.y * 1.5;
    var sum = vec3(0.0);
    for (var i = 0; i < 8; i++) {
        let turn = f32(i) * PI / 4.0;
        let at = uv + vec2(cos(turn), sin(turn)) * step;
        sum += textureSampleLevel(picture, picture_sampler, at, 0.0).rgb;
    }
    return sum / 8.0;
}

@fragment
fn fragment(mesh: VertexOutput) -> @location(0) vec4<f32> {
    var colour = blocks(mesh.uv);
    if look.x < 0.5 {
        return vec4(linear_to_srgb(colour), 1.0);
    }
    let crt = look.x > 1.5;
    if crt {
        let around = glow(mesh.uv);
        colour += GLOW * around * around;
    }
    var shown = linear_to_srgb(colour);
    let strength = select(SCANLINES, CRT_SCANLINES, crt);
    // The lines take away half their strength on the whole; half of that is given back.
    shown *= scanline(mesh.uv.y, strength) / (1.0 - strength * 0.25);
    if crt {
        shown *= mask(mesh.position.x);
        let from_middle = (mesh.uv - 0.5) * 2.0;
        let far = dot(from_middle, from_middle) / 2.0;
        shown *= 1.0 - CORNERS * far * far;
    }
    return vec4(clamp(shown, vec3(0.0), vec3(1.0)), 1.0);
}
