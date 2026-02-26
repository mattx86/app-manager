use std::path::Path;

fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default() != "windows" {
        return;
    }

    let out_dir = std::env::var("OUT_DIR").unwrap();
    let ico_path = Path::new(&out_dir).join("app.ico");
    let rgba_path = Path::new(&out_dir).join("icon_rgba.bin");

    // Generate multi-size ICO for the EXE resource
    let sizes: &[u32] = &[16, 32, 48, 64, 128, 256];
    let mut ico_entries: Vec<(u32, Vec<u8>)> = Vec::new();
    for &sz in sizes {
        let pixels = draw_icon(sz);
        let png = encode_png(&pixels, sz, sz);
        ico_entries.push((sz, png));
    }
    write_ico_multi(&ico_path, &ico_entries);

    // Generate 48x48 raw RGBA for runtime window/taskbar icon
    let rgba48 = draw_icon(48);
    std::fs::write(&rgba_path, &rgba48).expect("Failed to write icon_rgba.bin");

    let mut res = winresource::WindowsResource::new();
    res.set_icon(ico_path.to_str().unwrap());
    res.compile().unwrap();
}

// ── Icon drawing ─────────────────────────────────────────────────

/// Draw the wrench+gear icon at the target size using 4x supersampling.
fn draw_icon(size: u32) -> Vec<u8> {
    let scale = 4u32;
    let big = size * scale;
    let mut canvas = Canvas::new(big);
    let s = big as f32 / 64.0;

    // Colors
    let bg_top = [43, 74, 140, 255];        // #2b4a8c  dark blue
    let bg_bot = [91, 58, 156, 255];        // #5b3a9c  purple
    let gear_color = [220, 228, 240, 255];   // silver-white
    let gear_shadow = [100, 110, 140, 200];  // darker shadow
    let gear_hole = [55, 62, 140, 255];      // matches mid-background
    let wrench_body = [235, 165, 50, 255];   // warm amber/gold
    let wrench_dark = [190, 125, 30, 255];   // darker amber for shadow
    let wrench_light = [255, 210, 120, 180]; // highlight

    let pad = 2.0 * s;
    let corner_r = 10.0 * s;

    // --- Background: gradient-filled rounded square ---
    for row in (pad as u32)..(big - pad as u32) {
        let t = (row as f32 - pad) / (big as f32 - 2.0 * pad);
        let color = lerp_color(&bg_top, &bg_bot, t);
        for col in (pad as u32)..(big - pad as u32) {
            canvas.set(col, row, color);
        }
    }

    // Apply rounded-rect mask
    for y in 0..big {
        for x in 0..big {
            if !in_rounded_rect(x as f32, y as f32, pad, pad,
                                big as f32 - pad, big as f32 - pad, corner_r)
            {
                canvas.set(x, y, [0, 0, 0, 0]);
            }
        }
    }

    // Both gear and wrench centered at (32, 32) — true icon center
    let cx = 32.0_f32;
    let cy = 32.0_f32;

    // --- Gear (centered) ---
    let outer_r = 13.5;
    let inner_r = 10.0;
    let num_teeth: usize = 8;
    let tooth_half = std::f32::consts::PI / (num_teeth as f32 * 2.5);

    let mut gear_pts: Vec<(f32, f32)> = Vec::new();
    for i in 0..num_teeth {
        let base = (i as f32 / num_teeth as f32) * 2.0 * std::f32::consts::PI
            - std::f32::consts::PI / 2.0;

        // Tooth tip (outer)
        let a1 = base - tooth_half;
        gear_pts.push(((cx + outer_r * a1.cos()) * s, (cy + outer_r * a1.sin()) * s));
        let a2 = base + tooth_half;
        gear_pts.push(((cx + outer_r * a2.cos()) * s, (cy + outer_r * a2.sin()) * s));

        // Valley (inner)
        let valley = base + std::f32::consts::PI / num_teeth as f32;
        let v1 = valley - tooth_half * 0.6;
        gear_pts.push(((cx + inner_r * v1.cos()) * s, (cy + inner_r * v1.sin()) * s));
        let v2 = valley + tooth_half * 0.6;
        gear_pts.push(((cx + inner_r * v2.cos()) * s, (cy + inner_r * v2.sin()) * s));
    }

    // Gear shadow
    let shadow_pts: Vec<(f32, f32)> = gear_pts.iter()
        .map(|(x, y)| (x + 1.2 * s, y + 1.2 * s)).collect();
    canvas.fill_polygon(&shadow_pts, gear_shadow);

    // Gear body
    canvas.fill_polygon(&gear_pts, gear_color);

    // Gear center hole
    canvas.fill_circle(cx * s, cy * s, 4.5 * s, gear_hole);

    // --- Wrench (45° diagonal, centered on gear) ---
    let angle = 45.0_f32.to_radians();
    let cos_a = angle.cos();
    let sin_a = angle.sin();

    let rot = |x: f32, y: f32| -> (f32, f32) {
        let dx = x - cx;
        let dy = y - cy;
        ((cx + dx * cos_a - dy * sin_a) * s,
         (cy + dx * sin_a + dy * cos_a) * s)
    };

    let shadow_off = 0.9;
    let rot_s = |x: f32, y: f32| -> (f32, f32) {
        let dx = x - cx;
        let dy = y - cy;
        (((cx + dx * cos_a - dy * sin_a) + shadow_off) * s,
         ((cy + dx * sin_a + dy * cos_a) + shadow_off) * s)
    };

    // Double-ended half-hex wrench — hex socket opening at each end,
    // connected by a shaft. Rotated 45° across the gear.
    //
    //      ___/  \___       <- top head with hex notch opening up
    //     |          |
    //      \        /       <- taper to shaft
    //       |      |
    //       |      |        <- shaft
    //       |      |
    //      /        \       <- taper to bottom head
    //     |          |
    //      ___\  /___       <- bottom head with hex notch opening down
    //
    // Wrench shifted -0.5x, -0.5y from original to center on gear after rotation
    let wrench_outline: [(f32, f32); 20] = [
        // Top head with half-hex notch (opening faces up)
        (25.5,  6.5),    // top-left corner of head
        (28.5,  6.5),    // left edge of hex notch
        (30.0, 11.5),    // left inner facet of hex notch
        (33.0, 11.5),    // right inner facet of hex notch
        (34.5,  6.5),    // right edge of hex notch
        (37.5,  6.5),    // top-right corner of head
        // Right side going down
        (37.5, 14.5),    // right head end
        (33.5, 19.5),    // right taper into shaft
        (33.5, 43.5),    // right shaft end
        (37.5, 48.5),    // right taper into bottom head
        // Bottom head with half-hex notch (opening faces down)
        (37.5, 56.5),    // bottom-right corner of head
        (34.5, 56.5),    // right edge of bottom hex notch
        (33.0, 51.5),    // right inner facet of bottom hex notch
        (30.0, 51.5),    // left inner facet of bottom hex notch
        (28.5, 56.5),    // left edge of bottom hex notch
        (25.5, 56.5),    // bottom-left corner of head
        // Left side going up
        (25.5, 48.5),    // left bottom head top
        (29.5, 43.5),    // left taper from shaft
        (29.5, 19.5),    // left shaft top
        (25.5, 14.5),    // left head end
    ];

    // Shadow
    let shadow_pts: Vec<(f32, f32)> = wrench_outline.iter()
        .map(|&(x, y)| rot_s(x, y)).collect();
    canvas.fill_polygon(&shadow_pts, wrench_dark);

    // Body
    let body_pts: Vec<(f32, f32)> = wrench_outline.iter()
        .map(|&(x, y)| rot(x, y)).collect();
    canvas.fill_polygon(&body_pts, wrench_body);

    // Highlight along left edge of shaft + left side of heads
    canvas.fill_polygon(&[
        rot(25.5, 6.5), rot(26.7, 6.5), rot(26.7, 14.5), rot(25.5, 14.5),
    ], wrench_light);
    canvas.fill_polygon(&[
        rot(29.5, 19.5), rot(30.5, 19.5), rot(30.5, 43.5), rot(29.5, 43.5),
    ], wrench_light);
    canvas.fill_polygon(&[
        rot(25.5, 48.5), rot(26.7, 48.5), rot(26.7, 56.5), rot(25.5, 56.5),
    ], wrench_light);

    // --- Downsample 4x with box filter ---
    downsample(&canvas.pixels, big, scale)
}

// ── Canvas with drawing primitives ───────────────────────────────

struct Canvas {
    pixels: Vec<u8>,
    size: u32,
}

impl Canvas {
    fn new(size: u32) -> Self {
        Canvas {
            pixels: vec![0u8; (size * size * 4) as usize],
            size,
        }
    }

    fn set(&mut self, x: u32, y: u32, color: [u8; 4]) {
        if x < self.size && y < self.size {
            let idx = ((y * self.size + x) * 4) as usize;
            self.pixels[idx..idx + 4].copy_from_slice(&color);
        }
    }

    fn blend(&mut self, x: u32, y: u32, color: [u8; 4]) {
        if x >= self.size || y >= self.size {
            return;
        }
        let idx = ((y * self.size + x) * 4) as usize;
        let sa = color[3] as u32;
        if sa == 0 { return; }
        if sa == 255 {
            self.pixels[idx..idx + 4].copy_from_slice(&color);
            return;
        }
        let da = self.pixels[idx + 3] as u32;
        let inv_sa = 255 - sa;
        for c in 0..3 {
            self.pixels[idx + c] =
                ((color[c] as u32 * sa + self.pixels[idx + c] as u32 * inv_sa) / 255) as u8;
        }
        self.pixels[idx + 3] = (sa + da * inv_sa / 255) as u8;
    }

    fn fill_circle(&mut self, cx: f32, cy: f32, r: f32, color: [u8; 4]) {
        let r2 = r * r;
        let min_x = (cx - r).floor().max(0.0) as u32;
        let max_x = (cx + r).ceil().min(self.size as f32 - 1.0) as u32;
        let min_y = (cy - r).floor().max(0.0) as u32;
        let max_y = (cy + r).ceil().min(self.size as f32 - 1.0) as u32;
        for py in min_y..=max_y {
            for px in min_x..=max_x {
                let dx = px as f32 + 0.5 - cx;
                let dy = py as f32 + 0.5 - cy;
                if dx * dx + dy * dy <= r2 {
                    self.blend(px, py, color);
                }
            }
        }
    }

    fn fill_polygon(&mut self, pts: &[(f32, f32)], color: [u8; 4]) {
        if pts.is_empty() { return; }
        // Find bounding box
        let mut min_x = f32::MAX;
        let mut min_y = f32::MAX;
        let mut max_x = f32::MIN;
        let mut max_y = f32::MIN;
        for &(x, y) in pts {
            min_x = min_x.min(x);
            min_y = min_y.min(y);
            max_x = max_x.max(x);
            max_y = max_y.max(y);
        }
        let iy0 = min_y.floor().max(0.0) as u32;
        let iy1 = max_y.ceil().min(self.size as f32 - 1.0) as u32;
        let ix0 = min_x.floor().max(0.0) as u32;
        let ix1 = max_x.ceil().min(self.size as f32 - 1.0) as u32;

        // Scanline fill with point-in-polygon (ray casting)
        for py in iy0..=iy1 {
            for px in ix0..=ix1 {
                let fx = px as f32 + 0.5;
                let fy = py as f32 + 0.5;
                if point_in_polygon(fx, fy, pts) {
                    self.blend(px, py, color);
                }
            }
        }
    }
}

fn point_in_polygon(x: f32, y: f32, pts: &[(f32, f32)]) -> bool {
    let n = pts.len();
    let mut inside = false;
    let mut j = n - 1;
    for i in 0..n {
        let (xi, yi) = pts[i];
        let (xj, yj) = pts[j];
        if ((yi > y) != (yj > y)) && (x < (xj - xi) * (y - yi) / (yj - yi) + xi) {
            inside = !inside;
        }
        j = i;
    }
    inside
}

fn in_rounded_rect(x: f32, y: f32, x0: f32, y0: f32, x1: f32, y1: f32, r: f32) -> bool {
    if x < x0 || x > x1 || y < y0 || y > y1 {
        return false;
    }
    // Check corners
    let corners: [(f32, f32); 4] = [
        (x0 + r, y0 + r),  // top-left
        (x1 - r, y0 + r),  // top-right
        (x0 + r, y1 - r),  // bottom-left
        (x1 - r, y1 - r),  // bottom-right
    ];
    for &(cx, cy) in &corners {
        let in_corner_x = (x < x0 + r && cx == x0 + r) || (x > x1 - r && cx == x1 - r);
        let in_corner_y = (y < y0 + r && cy == y0 + r) || (y > y1 - r && cy == y1 - r);
        if in_corner_x && in_corner_y {
            let dx = x - cx;
            let dy = y - cy;
            return dx * dx + dy * dy <= r * r;
        }
    }
    true
}

fn lerp_color(a: &[u8; 4], b: &[u8; 4], t: f32) -> [u8; 4] {
    let t = t.clamp(0.0, 1.0);
    [
        (a[0] as f32 + (b[0] as f32 - a[0] as f32) * t) as u8,
        (a[1] as f32 + (b[1] as f32 - a[1] as f32) * t) as u8,
        (a[2] as f32 + (b[2] as f32 - a[2] as f32) * t) as u8,
        (a[3] as f32 + (b[3] as f32 - a[3] as f32) * t) as u8,
    ]
}

/// Box-filter downsample by `scale`x.
fn downsample(pixels: &[u8], big: u32, scale: u32) -> Vec<u8> {
    let small = big / scale;
    let mut out = vec![0u8; (small * small * 4) as usize];
    let count = (scale * scale) as u32;
    for sy in 0..small {
        for sx in 0..small {
            let mut r = 0u32;
            let mut g = 0u32;
            let mut b = 0u32;
            let mut a = 0u32;
            for dy in 0..scale {
                for dx in 0..scale {
                    let bx = sx * scale + dx;
                    let by = sy * scale + dy;
                    let idx = ((by * big + bx) * 4) as usize;
                    r += pixels[idx] as u32;
                    g += pixels[idx + 1] as u32;
                    b += pixels[idx + 2] as u32;
                    a += pixels[idx + 3] as u32;
                }
            }
            let oidx = ((sy * small + sx) * 4) as usize;
            out[oidx] = (r / count) as u8;
            out[oidx + 1] = (g / count) as u8;
            out[oidx + 2] = (b / count) as u8;
            out[oidx + 3] = (a / count) as u8;
        }
    }
    out
}

// ── PNG encoder (minimal, no dependencies) ───────────────────────

fn encode_png(rgba: &[u8], width: u32, height: u32) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&[137, 80, 78, 71, 13, 10, 26, 10]); // PNG signature

    // IHDR
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&width.to_be_bytes());
    ihdr.extend_from_slice(&height.to_be_bytes());
    ihdr.push(8); // bit depth
    ihdr.push(6); // RGBA
    ihdr.push(0); ihdr.push(0); ihdr.push(0);
    write_png_chunk(&mut out, b"IHDR", &ihdr);

    // IDAT
    let mut raw = Vec::new();
    for y in 0..height {
        raw.push(0); // filter: None
        let start = (y * width * 4) as usize;
        raw.extend_from_slice(&rgba[start..start + (width * 4) as usize]);
    }
    let compressed = zlib_compress_stored(&raw);
    write_png_chunk(&mut out, b"IDAT", &compressed);

    write_png_chunk(&mut out, b"IEND", &[]);
    out
}

fn write_png_chunk(out: &mut Vec<u8>, chunk_type: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    out.extend_from_slice(chunk_type);
    out.extend_from_slice(data);
    let mut crc_data = Vec::with_capacity(4 + data.len());
    crc_data.extend_from_slice(chunk_type);
    crc_data.extend_from_slice(data);
    out.extend_from_slice(&crc32(&crc_data).to_be_bytes());
}

fn zlib_compress_stored(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    out.push(0x78);
    out.push(0x01);
    let mut offset = 0;
    while offset < data.len() {
        let block_len = (data.len() - offset).min(65535);
        let is_last = offset + block_len >= data.len();
        out.push(if is_last { 0x01 } else { 0x00 });
        out.extend_from_slice(&(block_len as u16).to_le_bytes());
        out.extend_from_slice(&(!(block_len as u16)).to_le_bytes());
        out.extend_from_slice(&data[offset..offset + block_len]);
        offset += block_len;
    }
    out.extend_from_slice(&adler32(data).to_be_bytes());
    out
}

fn crc32(data: &[u8]) -> u32 {
    let mut crc: u32 = 0xFFFF_FFFF;
    for &byte in data {
        crc ^= byte as u32;
        for _ in 0..8 {
            crc = if crc & 1 != 0 { (crc >> 1) ^ 0xEDB8_8320 } else { crc >> 1 };
        }
    }
    !crc
}

fn adler32(data: &[u8]) -> u32 {
    let mut a: u32 = 1;
    let mut b: u32 = 0;
    for &byte in data {
        a = (a + byte as u32) % 65521;
        b = (b + a) % 65521;
    }
    (b << 16) | a
}

// ── ICO writer (multi-size, PNG-encoded entries) ─────────────────

fn write_ico_multi(path: &Path, entries: &[(u32, Vec<u8>)]) {
    let num = entries.len() as u16;
    let header_size = 6u32;
    let dir_entry_size = 16u32;
    let mut data_offset = header_size + dir_entry_size * num as u32;

    let mut ico = Vec::new();
    // Header
    ico.extend_from_slice(&0u16.to_le_bytes()); // reserved
    ico.extend_from_slice(&1u16.to_le_bytes()); // type: icon
    ico.extend_from_slice(&num.to_le_bytes());  // count

    // Directory entries
    for (size, png_data) in entries {
        let bw = if *size >= 256 { 0u8 } else { *size as u8 };
        ico.push(bw); // width
        ico.push(bw); // height
        ico.push(0);  // color palette
        ico.push(0);  // reserved
        ico.extend_from_slice(&1u16.to_le_bytes());  // color planes
        ico.extend_from_slice(&32u16.to_le_bytes()); // bits per pixel
        ico.extend_from_slice(&(png_data.len() as u32).to_le_bytes());
        ico.extend_from_slice(&data_offset.to_le_bytes());
        data_offset += png_data.len() as u32;
    }

    // PNG data
    for (_, png_data) in entries {
        ico.extend_from_slice(png_data);
    }

    std::fs::write(path, &ico).expect("Failed to write ICO file");
}
