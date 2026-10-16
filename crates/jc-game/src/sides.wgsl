// The blurred sides of the window (`display.rs`): the small picture stretched over the
// whole window, blurred once more by five by five samples of it weighed as a bell, and
// darkened, so that the sharp picture in the middle stands out. The camera composes in
// the stored colour values, so the result is turned back into them.

#import bevy_sprite::mesh2d_vertex_output::VertexOutput
#import bevy_render::color_operations::linear_to_srgb

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var small: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var small_sampler: sampler;
// xy: one texel of the small picture, z: how bright the sides are.
@group(#{MATERIAL_BIND_GROUP}) @binding(2) var<uniform> look: vec4<f32>;

@fragment
fn fragment(mesh: VertexOutput) -> @location(0) vec4<f32> {
    var weights = array<f32, 5>(1.0, 4.0, 6.0, 4.0, 1.0);
    var sum = vec3(0.0);
    for (var i = 0; i < 5; i++) {
        for (var j = 0; j < 5; j++) {
            let offset = vec2(f32(i - 2), f32(j - 2)) * look.xy;
            let at = clamp(mesh.uv + offset, vec2(0.0), vec2(1.0));
            let colour = textureSampleLevel(small, small_sampler, at, 0.0).rgb;
            sum += colour * weights[i] * weights[j];
        }
    }
    return vec4(linear_to_srgb(sum / 256.0) * look.z, 1.0);
}
