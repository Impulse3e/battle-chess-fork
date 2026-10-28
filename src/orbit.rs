//! Осмотр доски. Плоская картинка доски — это уже собранные клетки, её нельзя
//! честно повернуть. Здесь каждая клетка — блок: верх — ромб той же текстуры,
//! бока — земляные и серые, как в `tools/breach_art.py`. Фигуры стоят в той же
//! плоскости, что и обычная изометрия, и поворачиваются вместе с доской.
//! Обычный вид и ходы этим не затрагиваются.

use std::f32::consts::PI;
use std::ops::{Add, Mul, Sub};

use eframe::egui::epaint::{Mesh, Vertex};
use eframe::egui::{
    Align2, Color32, CornerRadius, FontId, Painter, Pos2, Rect, Shape, TextureId, Vec2, pos2, vec2,
};

use crate::assets::Textures;
use crate::game::{self, Color, Position, Square};

/// Высота блока в долях клетки. У арта бок — 31 пиксель при ширине ромба 56.
const HEIGHT: f32 = 0.56;
/// Куда смотрит камера: середина доски, чуть выше земли.
const TARGET: Vec3 = Vec3::new(4.0, 0.28, 4.0);
const DEFAULT_DISTANCE: f32 = 16.5;
const MIN_DISTANCE: f32 = 8.5;
const MAX_DISTANCE: f32 = 32.0;
const MIN_PITCH: f32 = 0.18;
const MAX_PITCH: f32 = 1.2;
/// Пикселей проекции на единицу `x/z`: доска занимает большую часть прямоугольника.
const FOCAL_SCALE: f32 = 1.48;

const BROWN_TOP: Color32 = Color32::from_rgb(0x7a, 0x6a, 0x52);
const BROWN_BOTTOM: Color32 = Color32::from_rgb(0x2a, 0x24, 0x20);
const BLUE_TOP: Color32 = Color32::from_rgb(0x4a, 0x5c, 0x64);
const BLUE_BOTTOM: Color32 = Color32::from_rgb(0x1a, 0x1e, 0x21);

/// Камера вокруг доски. Угол отсчитывается так, чтобы при входе осмотр совпал
/// с обычной изометрией: a1 слева, h1 ближе к зрителю.
#[derive(Clone, Copy)]
pub struct Orbit {
    yaw: f32,
    pitch: f32,
    distance: f32,
}

/// Что подсветить на верхних гранях, пока ходы не играются.
#[derive(Clone, Copy)]
pub struct Marks {
    pub hovered: Option<Square>,
    pub selected: Option<Square>,
    pub last_from: Option<Square>,
    pub last_to: Option<Square>,
    pub check: Option<Square>,
    pub threat: Option<Square>,
}

impl Orbit {
    /// Вид как у плоской доски. `flipped` — чёрные ближе к зрителю, как клавиша F.
    pub fn iso(flipped: bool) -> Self {
        let mut yaw = 135.0_f32.to_radians();
        if flipped {
            yaw += PI;
        }
        Self {
            yaw,
            pitch: 32.0_f32.to_radians(),
            distance: DEFAULT_DISTANCE,
        }
    }

    pub fn drag(&mut self, delta: Vec2) {
        self.yaw -= delta.x * 0.0075;
        self.pitch = (self.pitch - delta.y * 0.0055).clamp(MIN_PITCH, MAX_PITCH);
    }

    pub fn zoom(&mut self, scroll_y: f32) {
        self.distance = (self.distance - scroll_y * 0.012).clamp(MIN_DISTANCE, MAX_DISTANCE);
    }

    /// На пол-оборота: доска перевернулась той же клавишей, что и плоский вид.
    pub fn turn_around(&mut self) {
        self.yaw += PI;
    }

    fn eye(&self) -> Vec3 {
        let (sy, cy) = self.yaw.sin_cos();
        let (sp, cp) = self.pitch.sin_cos();
        TARGET + Vec3::new(sy * cp, sp, cy * cp) * self.distance
    }

    fn axes(&self) -> (Vec3, Vec3, Vec3, Vec3) {
        let eye = self.eye();
        let forward = (TARGET - eye).normalized();
        // `forward × вверх` смотрит вправо в правой системе, и a1 оказывается справа.
        // Экранный «вправо» берём противоположным: a1 слева, как на плоской доске.
        // Верх экрана при этом остаётся прежним (`forward × right`).
        let right = forward.cross(Vec3::Y).normalized() * -1.0;
        let up = forward.cross(right);
        (eye, forward, right, up)
    }

    /// Глубина точки: больше — дальше от камеры, такие грани рисуются раньше.
    fn depth(&self, world: Vec3) -> f32 {
        let (eye, forward, _, _) = self.axes();
        (world - eye).dot(forward)
    }

    fn project(&self, world: Vec3, viewport: Rect) -> Option<Pos2> {
        let (eye, forward, right, up) = self.axes();
        let rel = world - eye;
        let z = rel.dot(forward);
        if z < 0.2 {
            return None;
        }
        let focal = viewport.height().min(viewport.width()) * FOCAL_SCALE;
        let center = viewport.center();
        Some(center + vec2(rel.dot(right) / z * focal, -rel.dot(up) / z * focal))
    }

    /// Клетка, в чей верхний ромб попала точка экрана. Ближняя, если ромбы наложились.
    pub fn square_at(&self, pointer: Pos2, viewport: Rect) -> Option<Square> {
        let mut best: Option<(Square, f32)> = None;
        for rank in 0..8i8 {
            for file in 0..8i8 {
                let world = top_corners(file, rank);
                let mut screen = [Pos2::ZERO; 4];
                let mut visible = true;
                for (i, corner) in world.iter().enumerate() {
                    match self.project(*corner, viewport) {
                        Some(p) => screen[i] = p,
                        None => visible = false,
                    }
                }
                if !visible || !point_in_quad(pointer, screen) {
                    continue;
                }
                let depth = self.depth(top_center(file, rank));
                if best.is_none_or(|(_, so_far)| depth < so_far) {
                    let sq = game::square(file, rank)?;
                    best = Some((sq, depth));
                }
            }
        }
        best.map(|(sq, _)| sq)
    }
}

/// Объёмная доска в том же прямоугольнике, где лежит плоская картинка.
pub fn paint(
    painter: &Painter,
    viewport: Rect,
    tex: &Textures,
    orbit: Orbit,
    pos: &Position,
    marks: Marks,
    flipped: bool,
) {
    painter.rect_filled(
        viewport,
        CornerRadius::same(0),
        Color32::from_rgb(0x16, 0x15, 0x12),
    );
    let (eye, forward, right, _) = orbit.axes();
    let light = Vec3::new(0.45, 0.82, -0.35).normalized();
    // Пол рисуется первым и в сортировку не входит: иначе его центр закрыл бы дальние клетки.
    let mut floor = Vec::with_capacity(1);
    push_floor(&mut floor, orbit, viewport, tex.solid());
    for quad in &floor {
        paint_quad(painter, quad);
    }
    let mut quads = Vec::with_capacity(64 * 6 + 64);
    let mut figures = Vec::with_capacity(64);

    let barriers = pos.barrier(Color::White) | pos.barrier(Color::Black);
    let craters = pos.craters();
    for rank in 0..8i8 {
        for file in 0..8i8 {
            push_cell(
                &mut quads,
                orbit,
                viewport,
                tex,
                eye,
                light,
                file,
                rank,
                mark_color(game::square(file, rank).unwrap(), marks, barriers, craters),
            );
        }
    }
    for sq in 0..64u8 {
        let Some(piece) = pos.piece(sq) else {
            continue;
        };
        let feet = Vec3::new(
            game::file_of(sq) as f32 + 0.5,
            HEIGHT,
            game::rank_of(sq) as f32 + 0.5,
        );
        push_shadow(&mut quads, orbit, viewport, tex.solid(), feet, right, forward);
        let texture = if piece.frozen > 0 {
            tex.frozen(piece.color, piece.kind)
        } else {
            tex.piece_body(piece)
        };
        push_piece(&mut figures, orbit, viewport, texture, feet, flipped);
    }
    // Клетки — отдельно от фигур. Ближний бок блока иначе закрашивает фигуру за ним
    // целиком: у пешки макушка едва выше клетки.
    quads.sort_by(|a, b| a.depth.total_cmp(&b.depth).reverse());
    for quad in &quads {
        paint_quad(painter, quad);
    }
    figures.sort_by(|a, b| a.depth.total_cmp(&b.depth).reverse());
    for quad in &figures {
        paint_quad(painter, quad);
    }
    paint_corners(painter, viewport, orbit);
}

fn mark_color(sq: Square, marks: Marks, barriers: u64, craters: u64) -> Option<Color32> {
    let bit = 1_u64 << sq;
    if marks.hovered == Some(sq) {
        return Some(Color32::from_rgba_unmultiplied(255, 255, 255, 80));
    }
    if marks.selected == Some(sq) {
        return Some(Color32::from_rgba_unmultiplied(96, 156, 255, 115));
    }
    if marks.check == Some(sq) {
        return Some(Color32::from_rgba_unmultiplied(220, 48, 32, 125));
    }
    if marks.threat == Some(sq) {
        return Some(Color32::from_rgba_unmultiplied(255, 150, 48, 105));
    }
    if marks.last_from == Some(sq) || marks.last_to == Some(sq) {
        return Some(Color32::from_rgba_unmultiplied(236, 224, 176, 85));
    }
    if craters & bit != 0 {
        return Some(Color32::from_rgba_unmultiplied(255, 96, 32, 95));
    }
    if barriers & bit != 0 {
        return Some(Color32::from_rgba_unmultiplied(255, 196, 64, 75));
    }
    None
}

fn push_floor(quads: &mut Vec<Quad>, orbit: Orbit, viewport: Rect, texture: TextureId) {
    let y = -0.02;
    let world = [
        Vec3::new(-0.4, y, -0.4),
        Vec3::new(8.4, y, -0.4),
        Vec3::new(8.4, y, 8.4),
        Vec3::new(-0.4, y, 8.4),
    ];
    let color = Color32::from_rgb(0x10, 0x0f, 0x0d);
    push_world(quads, orbit, viewport, world, solid_uv(), [color; 4], texture);
}

fn push_cell(
    quads: &mut Vec<Quad>,
    orbit: Orbit,
    viewport: Rect,
    tex: &Textures,
    eye: Vec3,
    light: Vec3,
    file: i8,
    rank: i8,
    tint: Option<Color32>,
) {
    let (x0, x1, z0, z1) = cell_span(file, rank);
    // В обычном виде слева коричневый бок (грань большей вертикали), справа серый
    // (грань меньшей горизонтали) — как LEFT_STRATA и RIGHT_STRATA в генераторе.
    let sides = [
        (
            [
                Vec3::new(x0, 0.0, z0),
                Vec3::new(x1, 0.0, z0),
                Vec3::new(x1, HEIGHT, z0),
                Vec3::new(x0, HEIGHT, z0),
            ],
            Vec3::new(0.0, 0.0, -1.0),
            BLUE_TOP,
            BLUE_BOTTOM,
        ),
        (
            [
                Vec3::new(x1, 0.0, z1),
                Vec3::new(x0, 0.0, z1),
                Vec3::new(x0, HEIGHT, z1),
                Vec3::new(x1, HEIGHT, z1),
            ],
            Vec3::new(0.0, 0.0, 1.0),
            BLUE_TOP,
            BLUE_BOTTOM,
        ),
        (
            [
                Vec3::new(x0, 0.0, z1),
                Vec3::new(x0, 0.0, z0),
                Vec3::new(x0, HEIGHT, z0),
                Vec3::new(x0, HEIGHT, z1),
            ],
            Vec3::new(-1.0, 0.0, 0.0),
            BROWN_TOP,
            BROWN_BOTTOM,
        ),
        (
            [
                Vec3::new(x1, 0.0, z0),
                Vec3::new(x1, 0.0, z1),
                Vec3::new(x1, HEIGHT, z1),
                Vec3::new(x1, HEIGHT, z0),
            ],
            Vec3::new(1.0, 0.0, 0.0),
            BROWN_TOP,
            BROWN_BOTTOM,
        ),
    ];
    let solid = tex.solid();
    for (world, normal, top, bottom) in sides {
        let center = (world[0] + world[2]) * 0.5;
        if (eye - center).dot(normal) <= 0.0 {
            continue;
        }
        let shade = 0.52 + 0.48 * normal.dot(light).max(0.0);
        let colors = [
            bottom.linear_multiply(shade),
            bottom.linear_multiply(shade),
            top.linear_multiply(shade),
            top.linear_multiply(shade),
        ];
        push_world(quads, orbit, viewport, world, solid_uv(), colors, solid);
    }
    let top = top_corners(file, rank);
    let light_sq = (i32::from(file) + i32::from(rank)).rem_euclid(2) == 1;
    let variant = (i32::from(file) * 7 + i32::from(rank) * 3).rem_euclid(4) as usize;
    let shade = 0.62 + 0.38 * light.y.max(0.0);
    let color = Color32::from_rgb(
        (255.0 * shade) as u8,
        (255.0 * shade) as u8,
        (255.0 * shade) as u8,
    );
    push_world(
        quads,
        orbit,
        viewport,
        top,
        diamond_uvs(),
        [color; 4],
        tex.tile(light_sq, variant),
    );
    if let Some(tint) = tint {
        let raised = top.map(|p| Vec3::new(p.x, p.y + 0.02, p.z));
        push_world(
            quads,
            orbit,
            viewport,
            raised,
            solid_uv(),
            [tint; 4],
            solid,
        );
    }
}

fn push_shadow(
    quads: &mut Vec<Quad>,
    orbit: Orbit,
    viewport: Rect,
    texture: TextureId,
    feet: Vec3,
    right: Vec3,
    forward: Vec3,
) {
    let across = Vec3::new(right.x, 0.0, right.z).normalized();
    let along = Vec3::new(forward.x, 0.0, forward.z).normalized();
    let center = Vec3::new(feet.x, HEIGHT + 0.025, feet.z);
    let (rx, rz) = (0.34, 0.2);
    let world = [
        center - across * rx - along * rz,
        center + across * rx - along * rz,
        center + across * rx + along * rz,
        center - across * rx + along * rz,
    ];
    let color = Color32::from_rgba_unmultiplied(0, 0, 0, 88);
    push_world(quads, orbit, viewport, world, solid_uv(), [color; 4], texture);
}

/// 56 пикселей арта — одна клетка. Точка опоры спрайта — (32, 66) на холсте 64×84.
const SPRITE_HALF_W: f32 = 32.0 / 56.0;
const SPRITE_ABOVE: f32 = 66.0 / 56.0;
const SPRITE_BELOW: f32 = 18.0 / 56.0;

/// Карточка фигуры. В обычном ракурсе совпадает с камерой изометрии, поэтому
/// при входе в осмотр вид тот же. Дальше карточка стоит на месте в мире и
/// поворачивается вместе с доской. Почти в ребро она чуть доворачивается к
/// зрителю — иначе спрайт схлопывается в линию и фигура пропадает.
fn piece_axes(flipped: bool, orbit: Orbit) -> (Vec3, Vec3) {
    let home = Orbit::iso(flipped);
    let (_, home_forward, home_right, home_up) = home.axes();
    let (_, view_forward, view_right, view_up) = orbit.axes();
    let facing = view_forward.dot(home_forward);
    const KEEP: f32 = 0.5;
    if facing.abs() >= KEEP {
        return (home_right, home_up);
    }
    let t = ((KEEP - facing.abs()) / KEEP).clamp(0.0, 1.0);
    // Сзади «вправо» камеры зеркалит рисунок. Знак сохраняет лицо спрайта.
    let aim_right = if facing >= 0.0 {
        view_right
    } else {
        view_right * -1.0
    };
    let right = (home_right * (1.0 - t) + aim_right * t).normalized();
    let mut up = (home_up * (1.0 - t) + view_up * t).normalized();
    up = up - right * up.dot(right);
    if up.length() < 1.0e-4 {
        up = view_up;
    } else {
        up = up.normalized();
    }
    if up.dot(home_up) < 0.0 {
        up = up * -1.0;
    }
    (right, up)
}

fn sprite_corners(feet: Vec3, right: Vec3, up: Vec3) -> [Vec3; 4] {
    [
        feet - right * SPRITE_HALF_W + up * SPRITE_ABOVE,
        feet + right * SPRITE_HALF_W + up * SPRITE_ABOVE,
        feet + right * SPRITE_HALF_W - up * SPRITE_BELOW,
        feet - right * SPRITE_HALF_W - up * SPRITE_BELOW,
    ]
}

fn sprite_points(orbit: Orbit, viewport: Rect, feet: Vec3, flipped: bool) -> Option<[Pos2; 4]> {
    let (right, up) = piece_axes(flipped, orbit);
    let world = sprite_corners(feet, right, up);
    let mut points = [Pos2::ZERO; 4];
    let mut visible = true;
    for (i, corner) in world.iter().enumerate() {
        match orbit.project(*corner, viewport) {
            Some(point) => points[i] = point,
            None => visible = false,
        }
    }
    if visible {
        return Some(points);
    }
    // Один угол за камерой не должен стирать всю фигуру, если ноги ещё в кадре.
    let origin = orbit.project(feet, viewport)?;
    let z = orbit.depth(feet).max(0.35);
    let focal = viewport.height().min(viewport.width()) * FOCAL_SCALE;
    let unit = focal / z;
    let (_, _, cam_right, cam_up) = orbit.axes();
    let axis = |dir: Vec3, len: f32| {
        vec2(dir.dot(cam_right), -dir.dot(cam_up)) * (len * unit)
    };
    let side = axis(right, SPRITE_HALF_W);
    let above = axis(up, SPRITE_ABOVE);
    let below = axis(up, SPRITE_BELOW);
    Some([
        origin - side + above,
        origin + side + above,
        origin + side - below,
        origin - side - below,
    ])
}

fn push_piece(
    quads: &mut Vec<Quad>,
    orbit: Orbit,
    viewport: Rect,
    texture: TextureId,
    feet: Vec3,
    flipped: bool,
) {
    let Some(points) = sprite_points(orbit, viewport, feet, flipped) else {
        return;
    };
    quads.push(Quad {
        depth: orbit.depth(feet),
        texture,
        points,
        uvs: [pos2(0.0, 0.0), pos2(1.0, 0.0), pos2(1.0, 1.0), pos2(0.0, 1.0)],
        colors: [Color32::WHITE; 4],
    });
}

/// Углы доски подписаны, чтобы после поворота было видно, где a1.
fn paint_corners(painter: &Painter, viewport: Rect, orbit: Orbit) {
    let labels = [
        (Vec3::new(0.0, HEIGHT, 0.0), "a1"),
        (Vec3::new(8.0, HEIGHT, 0.0), "h1"),
        (Vec3::new(0.0, HEIGHT, 8.0), "a8"),
        (Vec3::new(8.0, HEIGHT, 8.0), "h8"),
    ];
    for (corner, text) in labels {
        let away = Vec3::new(corner.x - 4.0, 0.0, corner.z - 4.0).normalized();
        let Some(at) = orbit.project(corner + away * 0.42, viewport) else {
            continue;
        };
        painter.text(
            at + vec2(1.0, 1.0),
            Align2::CENTER_CENTER,
            text,
            FontId::proportional(13.0),
            Color32::from_rgb(0x14, 0x13, 0x11),
        );
        painter.text(
            at,
            Align2::CENTER_CENTER,
            text,
            FontId::proportional(13.0),
            Color32::from_rgb(0xe9, 0xec, 0xf2),
        );
    }
}

fn push_world(
    quads: &mut Vec<Quad>,
    orbit: Orbit,
    viewport: Rect,
    world: [Vec3; 4],
    uvs: [Pos2; 4],
    colors: [Color32; 4],
    texture: TextureId,
) {
    let mut points = [Pos2::ZERO; 4];
    let mut depth = 0.0;
    for i in 0..4 {
        let Some(p) = orbit.project(world[i], viewport) else {
            return;
        };
        points[i] = p;
        depth += orbit.depth(world[i]);
    }
    quads.push(Quad {
        depth: depth / 4.0,
        texture,
        points,
        uvs,
        colors,
    });
}

fn paint_quad(painter: &Painter, quad: &Quad) {
    let mut mesh = Mesh::with_texture(quad.texture);
    for i in 0..4 {
        mesh.vertices.push(Vertex {
            pos: quad.points[i],
            uv: quad.uvs[i],
            color: quad.colors[i],
        });
    }
    mesh.indices.extend_from_slice(&[0, 1, 2, 0, 2, 3]);
    painter.add(Shape::mesh(mesh));
}

/// Ромб верхней грани в текстуре клетки 56×74, в том же порядке, что углы клетки:
/// левый (file, rank), нижний (file+1, rank), правый (file+1, rank+1), верхний (file, rank+1).
fn diamond_uvs() -> [Pos2; 4] {
    let u = |x: f32, y: f32| pos2(x / 56.0, y / 74.0);
    [
        u(0.75, 21.0),
        u(28.0, 41.25),
        u(55.25, 21.0),
        u(28.0, 0.75),
    ]
}

fn solid_uv() -> [Pos2; 4] {
    [pos2(0.5, 0.5); 4]
}

fn cell_span(file: i8, rank: i8) -> (f32, f32, f32, f32) {
    let x0 = f32::from(file);
    let z0 = f32::from(rank);
    (x0, x0 + 1.0, z0, z0 + 1.0)
}

fn top_corners(file: i8, rank: i8) -> [Vec3; 4] {
    let (x0, x1, z0, z1) = cell_span(file, rank);
    [
        Vec3::new(x0, HEIGHT, z0),
        Vec3::new(x1, HEIGHT, z0),
        Vec3::new(x1, HEIGHT, z1),
        Vec3::new(x0, HEIGHT, z1),
    ]
}

fn top_center(file: i8, rank: i8) -> Vec3 {
    Vec3::new(f32::from(file) + 0.5, HEIGHT, f32::from(rank) + 0.5)
}

fn point_in_quad(p: Pos2, quad: [Pos2; 4]) -> bool {
    let mut sign = 0.0;
    for i in 0..4 {
        let a = quad[i];
        let b = quad[(i + 1) % 4];
        let cross = (b.x - a.x) * (p.y - a.y) - (b.y - a.y) * (p.x - a.x);
        if cross == 0.0 {
            continue;
        }
        if sign == 0.0 {
            sign = cross.signum();
        } else if sign * cross.signum() < 0.0 {
            return false;
        }
    }
    true
}

struct Quad {
    depth: f32,
    texture: TextureId,
    points: [Pos2; 4],
    uvs: [Pos2; 4],
    colors: [Color32; 4],
}

#[derive(Clone, Copy)]
struct Vec3 {
    x: f32,
    y: f32,
    z: f32,
}

impl Vec3 {
    const fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }

    const Y: Self = Self::new(0.0, 1.0, 0.0);

    fn dot(self, other: Self) -> f32 {
        self.x * other.x + self.y * other.y + self.z * other.z
    }

    fn cross(self, other: Self) -> Self {
        Self::new(
            self.y * other.z - self.z * other.y,
            self.z * other.x - self.x * other.z,
            self.x * other.y - self.y * other.x,
        )
    }

    fn length(self) -> f32 {
        self.dot(self).sqrt()
    }

    fn normalized(self) -> Self {
        let length = self.length();
        if length < 1.0e-6 {
            Self::Y
        } else {
            self * (1.0 / length)
        }
    }
}

impl Add for Vec3 {
    type Output = Self;

    fn add(self, rhs: Self) -> Self {
        Self::new(self.x + rhs.x, self.y + rhs.y, self.z + rhs.z)
    }
}

impl Sub for Vec3 {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self {
        Self::new(self.x - rhs.x, self.y - rhs.y, self.z - rhs.z)
    }
}

impl Mul<f32> for Vec3 {
    type Output = Self;

    fn mul(self, rhs: f32) -> Self {
        Self::new(self.x * rhs, self.y * rhs, self.z * rhs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn view() -> Rect {
        Rect::from_min_size(Pos2::ZERO, vec2(448.0, 368.0))
    }

    fn at(orbit: Orbit, file: i8, rank: i8) -> Pos2 {
        orbit.project(top_center(file, rank), view()).unwrap()
    }

    #[test]
    fn default_camera_matches_the_isometric_board() {
        let orbit = Orbit::iso(false);
        let a1 = at(orbit, 0, 0);
        let h1 = at(orbit, 7, 0);
        let a8 = at(orbit, 0, 7);
        let h8 = at(orbit, 7, 7);
        assert!(a1.x < h1.x, "a1 слева от h1");
        assert!(a1.x < h8.x, "a1 слева от h8");
        assert!(h1.y > a8.y, "h1 ближе и ниже на экране, чем a8");
        assert!(a1.y > a8.y, "a1 ниже a8");
        assert!(orbit.depth(top_center(7, 0)) < orbit.depth(top_center(0, 7)));
        for file in 0..8 {
            for rank in 0..8 {
                for corner in top_corners(file, rank) {
                    let p = orbit.project(corner, view()).unwrap();
                    assert!(p.x.is_finite() && p.y.is_finite());
                }
            }
        }
    }

    #[test]
    fn flipped_camera_brings_the_eighth_rank_forward() {
        let orbit = Orbit::iso(true);
        let a1 = at(orbit, 0, 0);
        let a8 = at(orbit, 0, 7);
        assert!(a8.y > a1.y, "после переворота a8 ближе");
        assert!(orbit.depth(top_center(0, 7)) < orbit.depth(top_center(0, 0)));
    }

    #[test]
    fn the_board_sits_inside_its_rectangle() {
        let orbit = Orbit::iso(false);
        let area = view();
        let mut min = pos2(f32::MAX, f32::MAX);
        let mut max = pos2(f32::MIN, f32::MIN);
        for (file, rank) in [(0, 0), (8, 0), (0, 8), (8, 8)] {
            let p = orbit
                .project(Vec3::new(file as f32, HEIGHT, rank as f32), area)
                .unwrap();
            min = min.min(p);
            max = max.max(p);
        }
        let width = (max.x - min.x) / area.width();
        let height = (max.y - min.y) / area.height();
        assert!(
            (0.72..1.02).contains(&width),
            "доля ширины {width:.3}, высоты {height:.3}"
        );
        assert!(
            (0.42..0.82).contains(&height),
            "доля ширины {width:.3}, высоты {height:.3}"
        );
    }

    #[test]
    fn picking_returns_the_square_under_the_pointer() {
        let area = view();
        for flipped in [false, true] {
            let mut orbit = Orbit::iso(flipped);
            for file in [0, 3, 7] {
                for rank in [0, 4, 7] {
                    let pointer = at(orbit, file, rank);
                    assert_eq!(orbit.square_at(pointer, area), game::square(file, rank));
                }
            }
            orbit.drag(vec2(80.0, -30.0));
            let pointer = at(orbit, 4, 4);
            assert_eq!(orbit.square_at(pointer, area), game::square(4, 4));
        }
    }

    fn span(points: [Pos2; 4]) -> (f32, f32) {
        let mut min = pos2(f32::MAX, f32::MAX);
        let mut max = pos2(f32::MIN, f32::MIN);
        for point in points {
            min = min.min(point);
            max = max.max(point);
        }
        (max.x - min.x, max.y - min.y)
    }

    #[test]
    fn pieces_turn_with_the_board_and_stay_visible() {
        let home = Orbit::iso(false);
        let feet = Vec3::new(4.5, HEIGHT, 4.5);
        let (right, up) = piece_axes(false, home);
        let (_, _, cam_right, cam_up) = home.axes();
        assert!(right.dot(cam_right) > 0.99, "на входе фигура смотрит как изометрия");
        assert!(up.dot(cam_up) > 0.99);

        let mut turned = home;
        turned.yaw += 0.65;
        let (right, _) = piece_axes(false, turned);
        let (_, _, home_right, _) = home.axes();
        let (_, _, cam_right, _) = turned.axes();
        assert!(
            right.dot(home_right) > 0.85,
            "после поворота карточка остаётся на доске"
        );
        assert!(
            right.dot(cam_right) < 0.9,
            "после поворота фигура не приклеена к камере"
        );

        let area = view();
        for step in 0..12 {
            let mut orbit = home;
            orbit.yaw += step as f32 * (PI / 6.0);
            orbit.pitch = 0.2 + (step % 5) as f32 * 0.2;
            let points = sprite_points(orbit, area, feet, false).expect("фигура в кадре");
            let (width, height) = span(points);
            let focal = area.width().min(area.height()) * FOCAL_SCALE;
            let face_w = (64.0 / 56.0) * focal / orbit.distance;
            let face_h = (84.0 / 56.0) * focal / orbit.distance;
            assert!(
                width > face_w * 0.28,
                "шаг {step}: ширина {width:.1} при фронте {face_w:.1}"
            );
            assert!(
                height > face_h * 0.28,
                "шаг {step}: высота {height:.1} при фронте {face_h:.1}"
            );
        }

        let mut low = Orbit::iso(false);
        low.pitch = MIN_PITCH;
        low.distance = MIN_DISTANCE;
        for file in 0..8 {
            for rank in 0..8 {
                let feet = Vec3::new(file as f32 + 0.5, HEIGHT, rank as f32 + 0.5);
                if low.project(feet, area).is_none() {
                    continue;
                }
                let points = sprite_points(low, area, feet, false).expect("фигура не пропадает");
                let (width, height) = span(points);
                assert!(
                    width > 4.0 && height > 4.0,
                    "клетка {file},{rank}: {width:.1}×{height:.1}"
                );
            }
        }
    }

    #[test]
    fn projection_stays_finite_as_the_camera_moves() {
        let area = view();
        for step in 0..8 {
            let mut orbit = Orbit::iso(false);
            orbit.yaw += step as f32 * 0.7;
            orbit.pitch = 0.25 + (step as f32 % 4.0) * 0.22;
            orbit.distance = 10.0 + step as f32;
            for file in 0..8 {
                for rank in 0..8 {
                    for corner in top_corners(file, rank) {
                        if let Some(p) = orbit.project(corner, area) {
                            assert!(p.x.is_finite() && p.y.is_finite());
                        }
                    }
                }
            }
        }
    }
}
