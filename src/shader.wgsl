struct VertexOutput {
	[[builtin(position)]] clip_position: vec4<f32>;
	[[location(1)]] color: vec4<f32>;
};

[[block]]
struct Params {
	size: vec2<u32>;
	offset: vec2<f32>;
	scale: f32;
	max_iterations: u32;
	padding: array<u32, 2>;
	color: vec4<f32>;
};
[[group(0), binding(0)]]
var<uniform> params: Params;

[[stage(vertex)]]
fn vs_main([[builtin(vertex_index)]] vertex_index: u32) -> VertexOutput {
	var out: VertexOutput;

	let screen_pos = vec2<f32>(f32(vertex_index % params.size.x), f32(vertex_index / params.size.x));

	out.clip_position = vec4<f32>(screen_pos / vec2<f32>(params.size) * 2.0 - 1.0, 0.0, 1.0);

	let y_invert_offset = vec2<f32>(params.offset.x, -params.offset.y);
	let pos = (screen_pos + y_invert_offset - vec2<f32>(params.size / 2u)) * params.scale;

	var z = vec2<f32>(0.0, 0.0);
	var i = 0u;
	for (; i < params.max_iterations && z.x * z.x + z.y * z.y <= 4.0; i = i + 1u) {
		z = vec2<f32>(z.x * z.x - z.y * z.y, z.x * z.y * 2.0) + pos;
	}
	if (i == params.max_iterations) {
		out.color = vec4<f32>(0.0, 0.0, 0.0, 1.0);
	} else {
		out.color = f32(i) / f32(params.max_iterations) * params.color;
	}

	return out;
}

[[stage(fragment)]]
fn fs_main(in: VertexOutput) -> [[location(0)]] vec4<f32> {
	return in.color;
}
