// A picture of a background layer: the part that is added to the picture behind it, or
// with `OVER` the part that lies over it. The blend state that does the adding is set
// where the material's pipeline is built (`field.rs`).

#import bevy_sprite::mesh2d_vertex_output::VertexOutput

#ifdef SRGB_OUTPUT
#import bevy_render::color_operations::linear_to_srgb
#endif

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var texture: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var texture_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(2) var<uniform> share: vec4<f32>;

@fragment
fn fragment(mesh: VertexOutput) -> @location(0) vec4<f32> {
    var color = textureSample(texture, texture_sampler, mesh.uv);
#ifdef SRGB_OUTPUT
    // The camera composes in the console's own colour values, not in linear light.
    color = vec4(linear_to_srgb(color.rgb), color.a);
#endif
#ifdef OVER
    // A picture that lies over the one behind it: the fourth share is how thick.
    return color * share;
#else
    // The brightness of the tiles, in the console's colour values.
    return vec4(color.rgb * share.rgb, color.a);
#endif
}
