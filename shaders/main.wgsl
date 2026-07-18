struct VertexOutput {
    @builtin(position) screen_position: vec4<f32>
};

@group(0) @binding(0) var<uniform> metadata: SDFMetadata;
@group(0) @binding(1) var<uniform> shapes: array<SDFShape, 1000>;
@group(0) @binding(2) var<uniform> styles: array<SDFStyle, 1000>;
@group(0) @binding(3) var<uniform> rectangles: array<SDFRectangle, 1000>;
@group(0) @binding(4) var<uniform> bezier: array<SDFBezier, 1000>;
@group(0) @binding(5) var<uniform> glyphs: array<SDFGlyph, 1000>;

@group(1) @binding(0) var ui_textures: binding_array<texture_2d<f32>>;
@group(1) @binding(1) var ui_sampler: sampler;

// Metadata required to draw 2D SDF shapes.
//
// - screen_dimensions: the width and height of the host screen
// - time: the current runtime of the app
struct SDFMetadata {
    screen_dimensions: vec2<f32>,
    time: f32,
    mode: u32
};

struct SDFShape {
    center: vec2<f32>,
    dimensions: vec2<f32>,
    shape_ty: u32,   // the type ID of the shape to generate
    looks_ptrs: u32, // first 16 bits: shape pointer, second 16 bits: style pointer
    next_ptrs: u32   // first 16 bits: next on layer pointer, second 16 bits: first child pointer
};

struct SDFStyle {
    primary_color: vec4<f32>,
    border_color: vec4<f32>,
    border_width: f32,
    texture_ptr: u32
};

struct SDFRectangle {
    radii: vec4<f32>
};

struct SDFBezier {
    a_off: vec2<f32>,
    b_off: vec2<f32>,
    c_off: vec2<f32>,
    thickness: f32
};

struct SDFGlyph {
    start_idx: u32,
    count: u32,
    _pad0: u32,
    _pad1: u32
};

fn hash_u32(x: u32) -> u32 {
    var h = x;
    h ^= h >> 16;
    h *= 0x45d9f3bu;
    h ^= h >> 16;
    return h;
}

fn shape_to_color(shape: SDFShape) -> vec4<f32> {
    // Mix several fields together to form a seed
    let seed = hash_u32(
        bitcast<u32>(shape.center.x * 1000.0)
        ^ hash_u32(bitcast<u32>(shape.center.y * 1000.0))
        ^ hash_u32(shape.shape_ty)
        ^ hash_u32(shape.looks_ptrs)
    );

    let r_bits = hash_u32(seed);
    let g_bits = hash_u32(seed ^ 0xdeadbeef);
    let b_bits = hash_u32(seed ^ 0xcafebabe);

    // Map u32 to [0.0, 1.0]
    let r = f32(r_bits) / f32(0xffffffffu);
    let g = f32(g_bits) / f32(0xffffffffu);
    let b = f32(b_bits) / f32(0xffffffffu);

    return vec4<f32>(r, g, b, 1.0);
}

fn walk_shape_tree(parent: SDFShape, point: vec2<f32>) -> vec4<f32> {
    // create and setup initial stack for tracking tree location
    var stack: array<u32, 128u>;
    var stack_length: i32 = 1;
    var count: i32 = 0;
    stack[0] = 0;

    // setup result tracking
    var color = vec4<f32>(0.0, 0.0, 0.0, 0.0);

    while stack_length > 0 {
        let current = stack[stack_length - 1];
        let shape = shapes[current];
        stack_length--;

        // check if the point is influenced by this shape
        let point_influence_shape = 
            point.x >= shape.dimensions.x / -2.0 + shape.center.x &&
            point.x <= shape.dimensions.x /  2.0 + shape.center.x &&
            point.y >= shape.dimensions.y / -2.0 + shape.center.y &&
            point.y <= shape.dimensions.y /  2.0 + shape.center.y;

        // unpack next pointers
        let next_ptr = shape.next_ptrs >> 16;
        let child_ptr = shape.next_ptrs & 0xFFFF;
    
        // if point is not in shape, simply advance to next and stop here
        if !point_influence_shape {
            if next_ptr != 0xFFFF {
                stack[stack_length] = next_ptr;
                stack_length++;
            }
            continue;
        }

        let shape_ptr = shape.looks_ptrs >> 16;
        let style_ptr = shape.looks_ptrs & 0xFFFF;
        let style = styles[style_ptr];

        // update with result with my color
        if metadata.mode == 0 {
            switch (shape.shape_ty) {
                case 1: {
                    let d = sdf_ellipse(point, shape.center, shape.dimensions / 2.0);
                    color = blend_shape(color, point, shape, style, d);
                }
                case 2: {
                    let rectangle = rectangles[shape_ptr];
                    let d = sdf_rectangle(point, shape.center, shape.dimensions * vec2<f32>(0.5, 0.5), rectangle.radii);
                    color = blend_shape(color, point, shape, style, d);
                }
                case 3: {
                    let bezier = bezier[shape_ptr];
                    let d = sdf_bezier(point, shape.center + bezier.a_off, shape.center + bezier.b_off, shape.center + bezier.c_off, bezier.thickness);
                    color = blend_shape(color, point, shape, style, d);
                }
                case 4: {
                    let glyph = glyphs[shape_ptr];
                    var min_dist2 = 1000000000.0;
                    var winding   = 0;

                    var i = 0u;
                    while (glyph.count > i) {
                        let curve = bezier[glyph.start_idx + i];
                        let A = curve.a_off + shape.center;
                        let B = curve.b_off + shape.center;
                        let C = curve.c_off + shape.center;

                        // Accumulate unsigned distance (squared, defer sqrt)
                        let d = sdf_bezier_dist2(point, A, B, C);
                        min_dist2 = min(min_dist2, d);

                        // Accumulate winding number
                        winding += bezier_winding(point, A, B, C);

                        i += 1;
                    }

                    let dist = sqrt(min_dist2);
                    let d = select(dist, -dist, winding != 0) - 0.5;
                    color = blend_shape(color, point, shape, style, d);
                    // color = vec4<f32>(-d, -d, -d, 1.0);
                }
                default: {}
            }
        } else if metadata.mode == 1 {
            color = shape_to_color(shape);
        }

        // add next to stack, then children so that children are run first
        if next_ptr != 0xFFFF {
            stack[stack_length] = next_ptr;
            stack_length++;
        }
        if child_ptr != 0xFFFF {
            stack[stack_length] = child_ptr;
            stack_length++;
        }
    }

    return color;
}

fn blend_shape(
    color: vec4<f32>,
    point: vec2<f32>,
    shape: SDFShape,
    style: SDFStyle,
    d: f32
) -> vec4<f32> {
    // calculate primary color with texture
    var primary_color = style.primary_color;
    if style.texture_ptr != 0xFFFFFFFFu {
        let uv = (point - shape.center + (shape.dimensions / 2.0)) / shape.dimensions;
        let tex_color = textureSample(ui_textures[style.texture_ptr], ui_sampler, uv);
        primary_color = tex_color * primary_color;
    }

    // if style.border_width > 5.0 { return vec4<f32>(0.0, 0.0, 0.0, 1.0); }
    // else { return vec4<f32>(1.0, 1.0, 1.0, 1.0); }

    // calculate local color with borders taken into account
    let border_mult = clamp(-d - style.border_width, 0.0, 1.0);
    // return vec4<f32>(vec3<f32>(border_mult), 1.0);
    var local_color = ((border_mult * primary_color) + ((1.0 - border_mult) * style.border_color)) * clamp(-d, 0.0, 1.0);
    // return local_color;
    // return style.border_color * (1.0 - border_mult);

    // handle alpha edge cases
    if local_color.a >= 1.0 { return local_color; }
    if local_color.a <= 0.0 { return color; }

    // add local color to output color, scaling for room left in alpha
    let mult = min(local_color.a, 1.0 - color.a);
    return color + vec4<f32>(local_color.r * local_color.a, local_color.g * local_color.a, local_color.b * local_color.a, local_color.a);
}

@fragment
fn fs_final(in: VertexOutput) -> @location(0) vec4<f32> {
    let point = in.screen_position.xy;

    let shape = shapes[0];
    var color = walk_shape_tree(shape, point);
    if color.a > 0.0 && color.a < 1.0 { 
        color.r *= color.a;
        color.g *= color.a;
        color.b *= color.a;
        color.a = 1.0;
    }
    return color;
}

fn sdf_circle(
    point: vec2<f32>,
    center: vec2<f32>,
    radius: f32
) -> f32 {
    return length(point - center) - radius;
}

fn sdf_ellipse(
    point: vec2<f32>,
    center: vec2<f32>,
    radius: vec2<f32>
) -> f32 {
    let p = point - center;
    let q = length(p / radius);
    return (q - 1.0) * min(radius.x, radius.y);
}

fn sdf_rectangle(
    point: vec2<f32>,
    center: vec2<f32>,
    size: vec2<f32>,
    radii: vec4<f32>
) -> f32 {
    var r = radii.x;
    r = select(r, radii.y, point.x < center.x);
    r = select(r, radii.z, point.y < center.y);
    r = select(r, radii.w, point.x < center.x && point.y < center.y);

    let d = abs(point - center) - size + r;
    return length(max(d, vec2<f32>(0.0))) + min(max(d.x, d.y), 0.0) - r;
}

fn dot2(v: vec2<f32>) -> f32 {
    return dot(v, v);
}

fn sdf_bezier(
    pos: vec2<f32>, 
    A: vec2<f32>, 
    B: vec2<f32>, 
    C: vec2<f32>,
    thickness: f32
) -> f32 {
    let a = B - A;
    let b = A - 2.0 * B + C;
    let c = a * 2.0;
    let d = A - pos;

    if 0.0001 > dot(b, b) {
        return sdf_line_segment(pos, A, C, thickness);
    }

    let kk = 1.0 / dot(b, b);
    let kx = kk * dot(a, b);
    let ky = kk * (2.0 * dot(a, a) + dot(d, b)) / 3.0;
    let kz = kk * dot(d, a);

    var res = 0.0;
    let p  = ky - kx * kx;
    let p3 = p * p * p;
    let q  = kx * (2.0 * kx * kx - 3.0 * ky) + kz;
    let h  = q * q + 4.0 * p3;

    if h >= 0.0 {
        let hs = sqrt(h);
        let x  = (vec2<f32>(hs, -hs) - q) / 2.0;
        let uv = sign(x) * pow(abs(x), vec2<f32>(1.0 / 3.0, 1.0 / 3.0));
        let t  = clamp(uv.x + uv.y - kx, 0.0, 1.0);
        res = dot2(d + (c + b * t) * t);
    } else {
        let z = sqrt(-p);
        let v = acos(clamp(q / (p * z * 2.0), -1.0, 1.0)) / 3.0;
        let m = cos(v);
        let n = sin(v) * 1.732050808;
        let t = clamp(vec3<f32>(m + m, -n - m, n - m) * z - kx, vec3<f32>(0.0), vec3<f32>(1.0));
        res = min(dot2(d + (c + b * t.x) * t.x),
                  dot2(d + (c + b * t.y) * t.y));
    }

    return sqrt(res) - thickness;
}

fn sdf_line_segment(
    pos: vec2<f32>,
    A: vec2<f32>,
    B: vec2<f32>,
    thickness: f32
) -> f32 {
    let pa = pos - A;
    let ba = B - A;
    let t = clamp(dot(pa, ba) / dot(ba, ba), 0.0, 1.0);
    return length(pa - ba * t) - thickness;
}

// Returns the winding contribution of one quadratic bezier against a point.
// Casts a ray in +X and counts signed crossings.
fn bezier_winding(pos: vec2<f32>, A: vec2<f32>, B: vec2<f32>, C: vec2<f32>) -> i32 {
    // Reframe: shift so pos is the origin, find where curve.y == 0
    // Quadratic coefficients for the Y component only
    let a = A.y - 2.0 * B.y + C.y;  // t^2 coefficient
    let b = 2.0 * (B.y - A.y);       // t^1 coefficient
    let c = A.y - pos.y;             // t^0 coefficient (shifted)

    var winding = 0;

    if 0.00001 > abs(a) {
        // Degenerate: linear in Y
        if 0.00001 < abs(b) {
            let t = -c / b;
            if t >= 0.0 && 1.0 > t {
                let x = mix(mix(A.x, B.x, t), mix(B.x, C.x, t), t);
                if x > pos.x {
                    // Determine crossing direction from dy/dt = b
                    winding += select(-1, 1, b > 0.0);
                }
            }
        }
        return winding;
    }

    let disc = b * b - 4.0 * a * c;
    if 0.0 > disc { return 0; }

    let sq = sqrt(disc);
    let t0 = (-b - sq) / (2.0 * a);
    let t1 = (-b + sq) / (2.0 * a);

    // For each root in [0, 1), check if crossing is to the right
    // Use half-open interval [0,1) to avoid double-counting shared endpoints
    var i = 0;
    while (2 > i) {
        let t = select(t0, t1, i == 1);
        if t >= 0.0 && 1.0 > t {
            let x = mix(mix(A.x, B.x, t), mix(B.x, C.x, t), t);
            if x > pos.x {
                // dy/dt at this t gives crossing direction
                let dy = 2.0 * a * t + b;
                winding += select(-1, 1, dy > 0.0);
            }
        }

        i += 1;
    }

    return winding;
}

fn sdf_bezier_dist2(
    pos: vec2<f32>,
    A: vec2<f32>,
    B: vec2<f32>,
    C: vec2<f32>,
) -> f32 {
    let a = B - A;
    let b = A - 2.0 * B + C;
    let c = a * 2.0;
    let d = A - pos;

    // Degenerate guard from before
    if 0.00001 > dot(b, b) {
        let pa = pos - A;
        let ba = C - A;
        let t = clamp(dot(pa, ba) / dot(ba, ba), 0.0, 1.0);
        let diff = pa - ba * t;
        return dot(diff, diff);  // squared distance to line segment
    }

    let kk = 1.0 / dot(b, b);
    let kx = kk * dot(a, b);
    let ky = kk * (2.0 * dot(a, a) + dot(d, b)) / 3.0;
    let kz = kk * dot(d, a);

    var res = 0.0;
    let p  = ky - kx * kx;
    let p3 = p * p * p;
    let q  = kx * (2.0 * kx * kx - 3.0 * ky) + kz;
    let h  = q * q + 4.0 * p3;

    if h >= 0.0 {
        let hs = sqrt(h);
        let x  = (vec2<f32>(hs, -hs) - q) / 2.0;
        let uv = sign(x) * pow(abs(x), vec2<f32>(1.0 / 3.0, 1.0 / 3.0));
        let t  = clamp(uv.x + uv.y - kx, 0.0, 1.0);
        res = dot2(d + (c + b * t) * t);
    } else {
        let z = sqrt(-p);
        let v = acos(clamp(q / (p * z * 2.0), -1.0, 1.0)) / 3.0;
        let m = cos(v);
        let n = sin(v) * 1.732050808;
        let t = clamp(vec3<f32>(m + m, -n - m, n - m) * z - kx, vec3<f32>(0.0), vec3<f32>(1.0));
        res = min(dot2(d + (c + b * t.x) * t.x),
                  dot2(d + (c + b * t.y) * t.y));
    }

    return res;  // <-- squared, no sqrt, no thickness
}
