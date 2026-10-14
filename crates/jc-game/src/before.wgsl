// The picture before (`before.rs`): the frame before laid over the new one by the three
// rules of `jc_formats::before::laid`, or a kept picture shown as it is. Both cameras that
// use this compose in the console's own colour values (`CompositingSpace::Srgb`), and the
// pictures are stored in those values, so a texel is turned back into them here. The alpha
// of a kept picture is the mask bit of its pixel.

#import bevy_sprite::mesh2d_vertex_output::VertexOutput
#import bevy_render::color_operations::linear_to_srgb

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var now_texture: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var now_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(2) var before_texture: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(3) var before_sampler: sampler;
// x: 0 lays the picture before over the new one, 1 shows the new one as it is.
@group(#{MATERIAL_BIND_GROUP}) @binding(4) var<uniform> way: vec4<f32>;

// The five bits a colour has in video memory, of a stored value between 0 and 1.
fn five_bits(stored: vec3<f32>) -> vec3<f32> {
    return floor(floor(stored * 255.0 + 0.5) / 8.0);
}

@fragment
fn fragment(mesh: VertexOutput) -> @location(0) vec4<f32> {
    let now = textureSample(now_texture, now_sampler, mesh.uv);
    let drawn = linear_to_srgb(now.rgb);
    if way.x > 0.5 {
        return vec4(drawn, 1.0);
    }
    let before = textureSample(before_texture, before_sampler, mesh.uv);
    let kept = linear_to_srgb(before.rgb);
    if before.a > 0.5 {
        // Marked: half and half, each colour in its five bits, widened as a texel is.
        let mixed = floor((five_bits(drawn) + five_bits(kept)) / 2.0);
        return vec4((mixed * 8.0 + floor(mixed / 4.0)) / 255.0, 1.0);
    }
    if all(five_bits(kept) == vec3(0.0)) {
        // Black without the mark is left out.
        return vec4(drawn, 0.0);
    }
    // Anything else is copied, and marked.
    return vec4(kept, 1.0);
}
