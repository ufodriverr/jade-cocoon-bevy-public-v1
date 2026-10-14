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

@fragment
fn fragment(mesh: VertexOutput) -> @location(0) vec4<f32> {
    let texel = textureSample(texture, texture_sampler, mesh.uv);
    // A texel that is not drawn, or that belongs to the other drawing step of the face.
    if texel.a < 0.5 {
        discard;
    }
    return vec4(texel.rgb * mesh.color.rgb, share.x);
}
