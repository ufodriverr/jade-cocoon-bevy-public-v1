// A piece of a thing a room shows, on the console's screen: the texels of a texture page
// times the colour of the corners, in the console's own colour values. The camera of the
// field composes in those values (`CompositingSpace::Srgb`) and the page is stored as they
// are, so nothing is converted here. The blend state that mixes a piece with what is behind
// it is set where the material's pipeline is built (`shown.rs`); `share` is how much of the
// texel that mixing takes.

#import bevy_sprite::mesh2d_vertex_output::VertexOutput

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var texture: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var texture_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(2) var<uniform> share: vec4<f32>;

// The console's dither (`dither` of `[display]` with `render = "authentic"`: the fourth
// share is 1). The offset of a 4 by 4 table by the pixel's place is added to the colour's
// eight bits, which then keep five, widened as a texel is (`before.wgsl`). The table is
// the console's GPU's. HYPOTHESIS: every piece of the 3D is dithered; which draw has the
// dither bit on is not read from the disc.
fn dithered(colour: vec3<f32>, at: vec2<f32>) -> vec3<f32> {
    var table = array<f32, 16>(
        -4.0, 0.0, -3.0, 1.0,
        2.0, -2.0, 3.0, -1.0,
        -3.0, 1.0, -4.0, 0.0,
        3.0, -1.0, 2.0, -2.0,
    );
    let cell = vec2<u32>(at) % vec2(4u);
    let eight = floor(colour * 255.0 + 0.5) + table[cell.y * 4u + cell.x];
    let five = floor(clamp(eight, vec3(0.0), vec3(255.0)) / 8.0);
    return (five * 8.0 + floor(five / 4.0)) / 255.0;
}

@fragment
fn fragment(mesh: VertexOutput) -> @location(0) vec4<f32> {
    let texel = textureSample(texture, texture_sampler, mesh.uv);
    // A texel that is not drawn, or that belongs to the other drawing step of the face.
    if texel.a < 0.5 {
        discard;
    }
    var colour = texel.rgb * mesh.color.rgb;
    if share.w > 0.5 {
        colour = dithered(colour, mesh.position.xy);
    }
    return vec4(colour, share.x);
}
