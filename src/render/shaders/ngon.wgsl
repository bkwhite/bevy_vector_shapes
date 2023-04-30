#import bevy_vector_shapes::core

struct Vertex {
    @builtin(vertex_index) index: u32,
    @location(0) instance_index: u32,
}

struct Ngon {
    @location(0) matrix_0: vec4<f32>,
    @location(1) matrix_1: vec4<f32>,
    @location(2) matrix_2: vec4<f32>,
    @location(3) matrix_3: vec4<f32>,

    @location(4) color: vec4<f32>,  
    @location(5) thickness: f32,
    @location(6) flags: u32,
  
    @location(7) sides: f32,
    @location(8) radius: f32,
    @location(9) roundness: f32
};

@group(1) @binding(0)
#ifdef BATCH_SIZE
var<uniform> shapes: array<Ngon, #{BATCH_SIZE}u>;
#else
var<storage> shapes: array<Ngon>;
#endif


struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) thickness: f32,
    @location(3) central_angle: f32,
    @location(4) half_side_length: f32,
    @location(5) roundness: f32
};

@vertex
fn vertex(v: Vertex) -> VertexOutput {
    var out: VertexOutput;

    // Vertex positions for a basic quad
    var vertexes: array<vec3<f32>, 6u> = array<vec3<f32>, 6u>(
        vec3<f32>(-1.0, 1.0, 0.0),
        vec3<f32>(1.0, 1.0, 0.0),
        vec3<f32>(1.0, -1.0, 0.0),
        vec3<f32>(1.0, -1.0, 0.0),
        vec3<f32>(-1.0, -1.0, 0.0),
        vec3<f32>(-1.0, 1.0, 0.0),
    );
    let vertex = vertexes[v.index % 6u];
    let shape = shapes[v.instance_index];

    // Reconstruct our transformation matrix
    let matrix = mat4x4<f32>(
        shape.matrix_0,
        shape.matrix_1,
        shape.matrix_2,
        shape.matrix_3
    );

    // Calculate vertex data shared between most shapes
    var vertex_data = get_vertex_data(matrix, vertex.xy * shape.radius, shape.thickness, shape.flags);
    out.clip_position = vertex_data.clip_pos;

    // Here we precompute several values related to our polygon

    // The central angle is the angle at the center of the polygon between two adjacent vertices
    // https://en.wikipedia.org/wiki/Central_angle
    out.central_angle = TAU / shape.sides; 

    // Calculate the apothem for a radius 1 polygon
    // The apothem is the length between the center of the polygon and a side at a right angle
    // https://en.wikipedia.org/wiki/Apothem
    var unit_apothem = cos(out.central_angle / 2.);

    // Calculate half of the side length for a radius 1 polygon
    var half_side_length = sin(out.central_angle / 2.);

    // Calculate our world space apothem for a polygon with the given radius
    var apothem = unit_apothem * shape.radius;

    // We want 1 unit in uv space to be the length of the apothem of our polygon 
    // so scale world to uv space using the world space apothem
    out.uv = vertex_data.local_pos / (apothem * vertex_data.scale) * vertex_data.uv_ratio;
    out.thickness = calculate_thickness(vertex_data.thickness_data, apothem, shape.flags);
    out.roundness = min(shape.roundness / apothem, 1.0);

    // Scale our half side length to match our uv space of 1 unit per apothem
    // Precalculate our scaling by the inverse of roundness for our sdf
    out.half_side_length = half_side_length / unit_apothem * (1.0 - out.roundness);

    out.color = shape.color;
    return out;
}

struct FragmentInput {
    @location(0) color: vec4<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) thickness: f32,
    @location(3) central_angle: f32,
    @location(4) half_side_length: f32,
    @location(5) roundness: f32
};

// Given a position, a central angle and a half side length determine the distance
//  between the point and a polygon with the given properties
fn ngonSDF(pos: vec2<f32>, central_angle: f32, half_side_length: f32, apothem: f32) -> f32 {
    // Rotate our position because pentagons look better when they point up :)
    var pos = pos.yx;

    // Calculate the angle between our point and positive y
    var angle = atan2(pos.y, pos.x);

    // Round the angle to the nearest vertex
    var nearest_angle = central_angle * floor((angle + 0.5 * central_angle) / central_angle);

    // Calculate the vector to that vertex
    var nearest_vertex = vec2<f32>(cos(nearest_angle), sin(nearest_angle));

    // Transform our point such that the x axis is along the apothem and the y axis is 
    //  along the side connected to the nearest vertex clockwise
    pos = mat2x2<f32>(nearest_vertex.x, -nearest_vertex.y, nearest_vertex.y, nearest_vertex.x) * pos;

    // The nearest point along the side to our point
    // Ensure that the y position falls along the length of the side
    var nearest_point = vec2<f32>(apothem, clamp(pos.y, -half_side_length, half_side_length));

    // Get the distance between our point and the nearest point on the side
    // If our x value is less than the apothem we fall inside the shape so multiply by -1
    return length(pos - nearest_point) * sign(pos.x - apothem);
}

// Due to https://github.com/gfx-rs/naga/issues/1743 this cannot be compiled into the vertex shader on web
#ifdef FRAGMENT
@fragment
fn fragment(f: FragmentInput) -> @location(0) vec4<f32> {
    // Mask representing whether this fragment falls within the shape
    var in_shape = f.color.a;

    // Calculate our positions distance from the polygon
    var dist = ngonSDF(f.uv, f.central_angle, f.half_side_length, 1.0 - f.roundness) - f.roundness;
    
    // Cut off points outside the shape or within the hollow area
    in_shape *= step_aa(-f.thickness, dist) * step_aa(dist, 0.);

    // Discard fragments no longer in the shape
    if in_shape < 0.0001 {
        discard;
    }

    return vec4<f32>(f.color.rgb, in_shape);
}
#endif