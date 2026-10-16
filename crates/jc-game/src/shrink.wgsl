// The picture of every screen shrunk to a small one for the blurred sides of the window
// (`display.rs`): each texel of the small picture is the mean of its part of the big one,
// six by six samples between its texels' corners, each the mean of four. The camera
// composes in the stored colour values, so the mean is turned back into them.

#import bevy_sprite::mesh2d_vertex_output::VertexOutput
#import bevy_render::color_operations::linear_to_srgb

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var picture: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var picture_sampler: sampler;
// xy: one texel of the small picture in the coordinates of the big one.
@group(#{MATERIAL_BIND_GROUP}) @binding(2) var<uniform> step: vec4<f32>;

const TAPS: i32 = 6;

@fragment
fn fragment(mesh: VertexOutput) -> @location(0) vec4<f32> {
    var sum = vec3(0.0);
    for (var i = 0; i < TAPS; i++) {
        for (var j = 0; j < TAPS; j++) {
            let offset = (vec2(f32(i), f32(j)) + 0.5) / f32(TAPS) - 0.5;
            let at = mesh.uv + offset * step.xy;
            sum += textureSampleLevel(picture, picture_sampler, at, 0.0).rgb;
        }
    }
    return vec4(linear_to_srgb(sum / f32(TAPS * TAPS)), 1.0);
}
