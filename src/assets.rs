//! Графика игры. Доска и фигуры — пиксель-арт в духе Into the Breach из `assets/breach`
//! (его рисует `tools/breach_art.py`): изометрические клетки-блоки с земляными боками,
//! белые — светлые мехи, чёрные — веки, у каждой фигуры отдельный слой тени. Здесь
//! картинка доски собирается из клеток вместе с буквами координат, рисуются подсветки
//! клеток, и всё увеличивается до текстур нужного размера. Значок окна — конь cburnett.

use std::cell::RefCell;
use std::rc::Rc;

use eframe::egui::{
    self, ColorImage, IconData, TextureFilter, TextureHandle, TextureId, TextureOptions, Vec2, vec2,
};
use resvg::{tiny_skia, usvg};

use crate::game::{Color, Kind, Piece};

/// Ромб верхней грани клетки, пиксели арта.
pub const TILE: Vec2 = vec2(56.0, 42.0);
/// Картинка доски: восемь ромбов по ширине, по высоте — до низа передней клетки.
pub const BOARD: Vec2 = vec2(448.0, 368.0);
/// Холст фигуры и точка на нём, которая ставится в центр клетки.
pub const SPRITE: Vec2 = vec2(64.0, 84.0);
pub const ANCHOR: Vec2 = vec2(32.0, 66.0);
/// Хитбокс фигуры (с 0.56, просьба пользователя) — силуэт, сжатый на столько пикселей арта со
/// всех сторон: на вид фигура та же, а соседние клетки ловят курсор у самого её края.
const HIT_SHRINK: usize = 2;
/// Строка холста, выше которой король не ловит курсор (с 0.56): высокий король закрывал
/// клетку позади по диагонали (на экране — прямо над ним, её центр — на строке 24).
const KING_HIT_TOP: usize = 38;

macro_rules! breach {
    ($($name:literal),* $(,)?) => {
        [$(include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/breach/", $name)) as &[u8]),*]
    };
}

/// Фигуры: белые пешка, конь, слон, ладья, ферзь, король, затем так же чёрные.
const BODIES: [&[u8]; 12] = breach!(
    "pieces/wP.png",
    "pieces/wN.png",
    "pieces/wB.png",
    "pieces/wR.png",
    "pieces/wQ.png",
    "pieces/wK.png",
    "pieces/bP.png",
    "pieces/bN.png",
    "pieces/bB.png",
    "pieces/bR.png",
    "pieces/bQ.png",
    "pieces/bK.png",
);
/// Фигуры в инее (`tools/breach_freeze.py`) — в том же порядке и на тех же холстах.
const FROZEN: [&[u8]; 12] = breach!(
    "pieces/frozen/wP.png",
    "pieces/frozen/wN.png",
    "pieces/frozen/wB.png",
    "pieces/frozen/wR.png",
    "pieces/frozen/wQ.png",
    "pieces/frozen/wK.png",
    "pieces/frozen/bP.png",
    "pieces/frozen/bN.png",
    "pieces/frozen/bB.png",
    "pieces/frozen/bR.png",
    "pieces/frozen/bQ.png",
    "pieces/frozen/bK.png",
);
/// Золотые фигуры rogue-like (с 0.68) — вариант `gold` из `tools/breach_gilded.py` (целиком
/// золотые), тот же холст и порядок, что у `BODIES`; тени — свои.
const GOLD_BODIES: [&[u8]; 12] = breach!(
    "pieces/gold/wP.png",
    "pieces/gold/wN.png",
    "pieces/gold/wB.png",
    "pieces/gold/wR.png",
    "pieces/gold/wQ.png",
    "pieces/gold/wK.png",
    "pieces/gold/bP.png",
    "pieces/gold/bN.png",
    "pieces/gold/bB.png",
    "pieces/gold/bR.png",
    "pieces/gold/bQ.png",
    "pieces/gold/bK.png",
);
const GOLD_SHADOWS: [&[u8]; 12] = breach!(
    "pieces/gold/wP_shadow.png",
    "pieces/gold/wN_shadow.png",
    "pieces/gold/wB_shadow.png",
    "pieces/gold/wR_shadow.png",
    "pieces/gold/wQ_shadow.png",
    "pieces/gold/wK_shadow.png",
    "pieces/gold/bP_shadow.png",
    "pieces/gold/bN_shadow.png",
    "pieces/gold/bB_shadow.png",
    "pieces/gold/bR_shadow.png",
    "pieces/gold/bQ_shadow.png",
    "pieces/gold/bK_shadow.png",
);
/// Ледяные фигуры rogue-like (с 0.72) — `tools/breach_ice_pieces.py`, порядок как у `BODIES`.
const ICE_BODIES: [&[u8]; 12] = breach!(
    "pieces/ice/wP.png",
    "pieces/ice/wN.png",
    "pieces/ice/wB.png",
    "pieces/ice/wR.png",
    "pieces/ice/wQ.png",
    "pieces/ice/wK.png",
    "pieces/ice/bP.png",
    "pieces/ice/bN.png",
    "pieces/ice/bB.png",
    "pieces/ice/bR.png",
    "pieces/ice/bQ.png",
    "pieces/ice/bK.png",
);
const ICE_SHADOWS: [&[u8]; 12] = breach!(
    "pieces/ice/wP_shadow.png",
    "pieces/ice/wN_shadow.png",
    "pieces/ice/wB_shadow.png",
    "pieces/ice/wR_shadow.png",
    "pieces/ice/wQ_shadow.png",
    "pieces/ice/wK_shadow.png",
    "pieces/ice/bP_shadow.png",
    "pieces/ice/bN_shadow.png",
    "pieces/ice/bB_shadow.png",
    "pieces/ice/bR_shadow.png",
    "pieces/ice/bQ_shadow.png",
    "pieces/ice/bK_shadow.png",
);
/// Ядовитые фигуры волн rogue-like (с 0.79) — `tools/breach_poison.py`, порядок как у `BODIES`.
const POISON_BODIES: [&[u8]; 12] = breach!(
    "pieces/poison/wP.png",
    "pieces/poison/wN.png",
    "pieces/poison/wB.png",
    "pieces/poison/wR.png",
    "pieces/poison/wQ.png",
    "pieces/poison/wK.png",
    "pieces/poison/bP.png",
    "pieces/poison/bN.png",
    "pieces/poison/bB.png",
    "pieces/poison/bR.png",
    "pieces/poison/bQ.png",
    "pieces/poison/bK.png",
);
const POISON_SHADOWS: [&[u8]; 12] = breach!(
    "pieces/poison/wP_shadow.png",
    "pieces/poison/wN_shadow.png",
    "pieces/poison/wB_shadow.png",
    "pieces/poison/wR_shadow.png",
    "pieces/poison/wQ_shadow.png",
    "pieces/poison/wK_shadow.png",
    "pieces/poison/bP_shadow.png",
    "pieces/poison/bN_shadow.png",
    "pieces/poison/bB_shadow.png",
    "pieces/poison/bR_shadow.png",
    "pieces/poison/bQ_shadow.png",
    "pieces/poison/bK_shadow.png",
);
/// Варвары rogue-like (с 0.80) — `tools/breach_barbarian.py`, порядок как у `BODIES`.
const BARBARIAN_BODIES: [&[u8]; 12] = breach!(
    "pieces/barbarian/wP.png",
    "pieces/barbarian/wN.png",
    "pieces/barbarian/wB.png",
    "pieces/barbarian/wR.png",
    "pieces/barbarian/wQ.png",
    "pieces/barbarian/wK.png",
    "pieces/barbarian/bP.png",
    "pieces/barbarian/bN.png",
    "pieces/barbarian/bB.png",
    "pieces/barbarian/bR.png",
    "pieces/barbarian/bQ.png",
    "pieces/barbarian/bK.png",
);
const BARBARIAN_SHADOWS: [&[u8]; 12] = breach!(
    "pieces/barbarian/wP_shadow.png",
    "pieces/barbarian/wN_shadow.png",
    "pieces/barbarian/wB_shadow.png",
    "pieces/barbarian/wR_shadow.png",
    "pieces/barbarian/wQ_shadow.png",
    "pieces/barbarian/wK_shadow.png",
    "pieces/barbarian/bP_shadow.png",
    "pieces/barbarian/bN_shadow.png",
    "pieces/barbarian/bB_shadow.png",
    "pieces/barbarian/bR_shadow.png",
    "pieces/barbarian/bQ_shadow.png",
    "pieces/barbarian/bK_shadow.png",
);
/// Маги rogue-like (с 0.83) — `tools/breach_wizard.py`, порядок как у `BODIES`.
const WIZARD_BODIES: [&[u8]; 12] = breach!(
    "pieces/wizard/wP.png",
    "pieces/wizard/wN.png",
    "pieces/wizard/wB.png",
    "pieces/wizard/wR.png",
    "pieces/wizard/wQ.png",
    "pieces/wizard/wK.png",
    "pieces/wizard/bP.png",
    "pieces/wizard/bN.png",
    "pieces/wizard/bB.png",
    "pieces/wizard/bR.png",
    "pieces/wizard/bQ.png",
    "pieces/wizard/bK.png",
);
const WIZARD_SHADOWS: [&[u8]; 12] = breach!(
    "pieces/wizard/wP_shadow.png",
    "pieces/wizard/wN_shadow.png",
    "pieces/wizard/wB_shadow.png",
    "pieces/wizard/wR_shadow.png",
    "pieces/wizard/wQ_shadow.png",
    "pieces/wizard/wK_shadow.png",
    "pieces/wizard/bP_shadow.png",
    "pieces/wizard/bN_shadow.png",
    "pieces/wizard/bB_shadow.png",
    "pieces/wizard/bR_shadow.png",
    "pieces/wizard/bQ_shadow.png",
    "pieces/wizard/bK_shadow.png",
);
/// Бронированные фигуры волн rogue-like (с 0.89) — `tools/breach_armor.py`, порядок как у `BODIES`.
const ARMOR_BODIES: [&[u8]; 12] = breach!(
    "pieces/armor/wP.png",
    "pieces/armor/wN.png",
    "pieces/armor/wB.png",
    "pieces/armor/wR.png",
    "pieces/armor/wQ.png",
    "pieces/armor/wK.png",
    "pieces/armor/bP.png",
    "pieces/armor/bN.png",
    "pieces/armor/bB.png",
    "pieces/armor/bR.png",
    "pieces/armor/bQ.png",
    "pieces/armor/bK.png",
);
const ARMOR_SHADOWS: [&[u8]; 12] = breach!(
    "pieces/armor/wP_shadow.png",
    "pieces/armor/wN_shadow.png",
    "pieces/armor/wB_shadow.png",
    "pieces/armor/wR_shadow.png",
    "pieces/armor/wQ_shadow.png",
    "pieces/armor/wK_shadow.png",
    "pieces/armor/bP_shadow.png",
    "pieces/armor/bN_shadow.png",
    "pieces/armor/bB_shadow.png",
    "pieces/armor/bR_shadow.png",
    "pieces/armor/bQ_shadow.png",
    "pieces/armor/bK_shadow.png",
);
/// Фигуры-вампиры волн rogue-like (с 0.89) — вариант «Дракула» из `tools/breach_vampire.py`.
const VAMPIRE_BODIES: [&[u8]; 12] = breach!(
    "pieces/vampire_dracula/wP.png",
    "pieces/vampire_dracula/wN.png",
    "pieces/vampire_dracula/wB.png",
    "pieces/vampire_dracula/wR.png",
    "pieces/vampire_dracula/wQ.png",
    "pieces/vampire_dracula/wK.png",
    "pieces/vampire_dracula/bP.png",
    "pieces/vampire_dracula/bN.png",
    "pieces/vampire_dracula/bB.png",
    "pieces/vampire_dracula/bR.png",
    "pieces/vampire_dracula/bQ.png",
    "pieces/vampire_dracula/bK.png",
);
const VAMPIRE_SHADOWS: [&[u8]; 12] = breach!(
    "pieces/vampire_dracula/wP_shadow.png",
    "pieces/vampire_dracula/wN_shadow.png",
    "pieces/vampire_dracula/wB_shadow.png",
    "pieces/vampire_dracula/wR_shadow.png",
    "pieces/vampire_dracula/wQ_shadow.png",
    "pieces/vampire_dracula/wK_shadow.png",
    "pieces/vampire_dracula/bP_shadow.png",
    "pieces/vampire_dracula/bN_shadow.png",
    "pieces/vampire_dracula/bB_shadow.png",
    "pieces/vampire_dracula/bR_shadow.png",
    "pieces/vampire_dracula/bQ_shadow.png",
    "pieces/vampire_dracula/bK_shadow.png",
);
/// Вампиры в броне — волны rogue-like (с 0.106), `tools/breach_armor_vampire.py`.
const ARMOR_VAMPIRE_BODIES: [&[u8]; 12] = breach!(
    "pieces/armor_vampire/wP.png",
    "pieces/armor_vampire/wN.png",
    "pieces/armor_vampire/wB.png",
    "pieces/armor_vampire/wR.png",
    "pieces/armor_vampire/wQ.png",
    "pieces/armor_vampire/wK.png",
    "pieces/armor_vampire/bP.png",
    "pieces/armor_vampire/bN.png",
    "pieces/armor_vampire/bB.png",
    "pieces/armor_vampire/bR.png",
    "pieces/armor_vampire/bQ.png",
    "pieces/armor_vampire/bK.png",
);
const ARMOR_VAMPIRE_SHADOWS: [&[u8]; 12] = breach!(
    "pieces/armor_vampire/wP_shadow.png",
    "pieces/armor_vampire/wN_shadow.png",
    "pieces/armor_vampire/wB_shadow.png",
    "pieces/armor_vampire/wR_shadow.png",
    "pieces/armor_vampire/wQ_shadow.png",
    "pieces/armor_vampire/wK_shadow.png",
    "pieces/armor_vampire/bP_shadow.png",
    "pieces/armor_vampire/bN_shadow.png",
    "pieces/armor_vampire/bB_shadow.png",
    "pieces/armor_vampire/bR_shadow.png",
    "pieces/armor_vampire/bQ_shadow.png",
    "pieces/armor_vampire/bK_shadow.png",
);
/// Ядовитые зомби — волны rogue-like (с 0.107), `tools/breach_poison_zombie.py`.
const POISON_ZOMBIE_BODIES: [&[u8]; 12] = breach!(
    "pieces/poison_zombie/wP.png",
    "pieces/poison_zombie/wN.png",
    "pieces/poison_zombie/wB.png",
    "pieces/poison_zombie/wR.png",
    "pieces/poison_zombie/wQ.png",
    "pieces/poison_zombie/wK.png",
    "pieces/poison_zombie/bP.png",
    "pieces/poison_zombie/bN.png",
    "pieces/poison_zombie/bB.png",
    "pieces/poison_zombie/bR.png",
    "pieces/poison_zombie/bQ.png",
    "pieces/poison_zombie/bK.png",
);
const POISON_ZOMBIE_SHADOWS: [&[u8]; 12] = breach!(
    "pieces/poison_zombie/wP_shadow.png",
    "pieces/poison_zombie/wN_shadow.png",
    "pieces/poison_zombie/wB_shadow.png",
    "pieces/poison_zombie/wR_shadow.png",
    "pieces/poison_zombie/wQ_shadow.png",
    "pieces/poison_zombie/wK_shadow.png",
    "pieces/poison_zombie/bP_shadow.png",
    "pieces/poison_zombie/bN_shadow.png",
    "pieces/poison_zombie/bB_shadow.png",
    "pieces/poison_zombie/bR_shadow.png",
    "pieces/poison_zombie/bQ_shadow.png",
    "pieces/poison_zombie/bK_shadow.png",
);
/// Фигуры-зомби волн rogue-like (с 0.105) — `tools/breach_zombie.py`.
const ZOMBIE_BODIES: [&[u8]; 12] = breach!(
    "pieces/zombie/wP.png",
    "pieces/zombie/wN.png",
    "pieces/zombie/wB.png",
    "pieces/zombie/wR.png",
    "pieces/zombie/wQ.png",
    "pieces/zombie/wK.png",
    "pieces/zombie/bP.png",
    "pieces/zombie/bN.png",
    "pieces/zombie/bB.png",
    "pieces/zombie/bR.png",
    "pieces/zombie/bQ.png",
    "pieces/zombie/bK.png",
);
const ZOMBIE_SHADOWS: [&[u8]; 12] = breach!(
    "pieces/zombie/wP_shadow.png",
    "pieces/zombie/wN_shadow.png",
    "pieces/zombie/wB_shadow.png",
    "pieces/zombie/wR_shadow.png",
    "pieces/zombie/wQ_shadow.png",
    "pieces/zombie/wK_shadow.png",
    "pieces/zombie/bP_shadow.png",
    "pieces/zombie/bN_shadow.png",
    "pieces/zombie/bB_shadow.png",
    "pieces/zombie/bR_shadow.png",
    "pieces/zombie/bQ_shadow.png",
    "pieces/zombie/bK_shadow.png",
);
/// Объединённые фигуры со своими спрайтами (с 0.96, `tools/breach_barbarian_combo.py` и
/// `tools/breach_wizard_horns.py`): золотой и ледяной варвар, варвар-маг — тела, потом тени.
const BARBARIAN_GOLD: [&[u8]; 24] = breach!(
    "pieces/barbarian_gold/wP.png",
    "pieces/barbarian_gold/wN.png",
    "pieces/barbarian_gold/wB.png",
    "pieces/barbarian_gold/wR.png",
    "pieces/barbarian_gold/wQ.png",
    "pieces/barbarian_gold/wK.png",
    "pieces/barbarian_gold/bP.png",
    "pieces/barbarian_gold/bN.png",
    "pieces/barbarian_gold/bB.png",
    "pieces/barbarian_gold/bR.png",
    "pieces/barbarian_gold/bQ.png",
    "pieces/barbarian_gold/bK.png",
    "pieces/barbarian_gold/wP_shadow.png",
    "pieces/barbarian_gold/wN_shadow.png",
    "pieces/barbarian_gold/wB_shadow.png",
    "pieces/barbarian_gold/wR_shadow.png",
    "pieces/barbarian_gold/wQ_shadow.png",
    "pieces/barbarian_gold/wK_shadow.png",
    "pieces/barbarian_gold/bP_shadow.png",
    "pieces/barbarian_gold/bN_shadow.png",
    "pieces/barbarian_gold/bB_shadow.png",
    "pieces/barbarian_gold/bR_shadow.png",
    "pieces/barbarian_gold/bQ_shadow.png",
    "pieces/barbarian_gold/bK_shadow.png",
);
const BARBARIAN_ICE: [&[u8]; 24] = breach!(
    "pieces/barbarian_ice/wP.png",
    "pieces/barbarian_ice/wN.png",
    "pieces/barbarian_ice/wB.png",
    "pieces/barbarian_ice/wR.png",
    "pieces/barbarian_ice/wQ.png",
    "pieces/barbarian_ice/wK.png",
    "pieces/barbarian_ice/bP.png",
    "pieces/barbarian_ice/bN.png",
    "pieces/barbarian_ice/bB.png",
    "pieces/barbarian_ice/bR.png",
    "pieces/barbarian_ice/bQ.png",
    "pieces/barbarian_ice/bK.png",
    "pieces/barbarian_ice/wP_shadow.png",
    "pieces/barbarian_ice/wN_shadow.png",
    "pieces/barbarian_ice/wB_shadow.png",
    "pieces/barbarian_ice/wR_shadow.png",
    "pieces/barbarian_ice/wQ_shadow.png",
    "pieces/barbarian_ice/wK_shadow.png",
    "pieces/barbarian_ice/bP_shadow.png",
    "pieces/barbarian_ice/bN_shadow.png",
    "pieces/barbarian_ice/bB_shadow.png",
    "pieces/barbarian_ice/bR_shadow.png",
    "pieces/barbarian_ice/bQ_shadow.png",
    "pieces/barbarian_ice/bK_shadow.png",
);
const WIZARD_HORNS: [&[u8]; 24] = breach!(
    "pieces/wizard_horns/wP.png",
    "pieces/wizard_horns/wN.png",
    "pieces/wizard_horns/wB.png",
    "pieces/wizard_horns/wR.png",
    "pieces/wizard_horns/wQ.png",
    "pieces/wizard_horns/wK.png",
    "pieces/wizard_horns/bP.png",
    "pieces/wizard_horns/bN.png",
    "pieces/wizard_horns/bB.png",
    "pieces/wizard_horns/bR.png",
    "pieces/wizard_horns/bQ.png",
    "pieces/wizard_horns/bK.png",
    "pieces/wizard_horns/wP_shadow.png",
    "pieces/wizard_horns/wN_shadow.png",
    "pieces/wizard_horns/wB_shadow.png",
    "pieces/wizard_horns/wR_shadow.png",
    "pieces/wizard_horns/wQ_shadow.png",
    "pieces/wizard_horns/wK_shadow.png",
    "pieces/wizard_horns/bP_shadow.png",
    "pieces/wizard_horns/bN_shadow.png",
    "pieces/wizard_horns/bB_shadow.png",
    "pieces/wizard_horns/bR_shadow.png",
    "pieces/wizard_horns/bQ_shadow.png",
    "pieces/wizard_horns/bK_shadow.png",
);
/// Ледяная золотая фигура и золотой маг (с 0.97, `tools/breach_gold_combo.py`) — тела, потом тени.
const GOLD_ICE: [&[u8]; 24] = breach!(
    "pieces/gold_ice/wP.png",
    "pieces/gold_ice/wN.png",
    "pieces/gold_ice/wB.png",
    "pieces/gold_ice/wR.png",
    "pieces/gold_ice/wQ.png",
    "pieces/gold_ice/wK.png",
    "pieces/gold_ice/bP.png",
    "pieces/gold_ice/bN.png",
    "pieces/gold_ice/bB.png",
    "pieces/gold_ice/bR.png",
    "pieces/gold_ice/bQ.png",
    "pieces/gold_ice/bK.png",
    "pieces/gold_ice/wP_shadow.png",
    "pieces/gold_ice/wN_shadow.png",
    "pieces/gold_ice/wB_shadow.png",
    "pieces/gold_ice/wR_shadow.png",
    "pieces/gold_ice/wQ_shadow.png",
    "pieces/gold_ice/wK_shadow.png",
    "pieces/gold_ice/bP_shadow.png",
    "pieces/gold_ice/bN_shadow.png",
    "pieces/gold_ice/bB_shadow.png",
    "pieces/gold_ice/bR_shadow.png",
    "pieces/gold_ice/bQ_shadow.png",
    "pieces/gold_ice/bK_shadow.png",
);
const GOLD_WIZARD: [&[u8]; 24] = breach!(
    "pieces/gold_wizard/wP.png",
    "pieces/gold_wizard/wN.png",
    "pieces/gold_wizard/wB.png",
    "pieces/gold_wizard/wR.png",
    "pieces/gold_wizard/wQ.png",
    "pieces/gold_wizard/wK.png",
    "pieces/gold_wizard/bP.png",
    "pieces/gold_wizard/bN.png",
    "pieces/gold_wizard/bB.png",
    "pieces/gold_wizard/bR.png",
    "pieces/gold_wizard/bQ.png",
    "pieces/gold_wizard/bK.png",
    "pieces/gold_wizard/wP_shadow.png",
    "pieces/gold_wizard/wN_shadow.png",
    "pieces/gold_wizard/wB_shadow.png",
    "pieces/gold_wizard/wR_shadow.png",
    "pieces/gold_wizard/wQ_shadow.png",
    "pieces/gold_wizard/wK_shadow.png",
    "pieces/gold_wizard/bP_shadow.png",
    "pieces/gold_wizard/bN_shadow.png",
    "pieces/gold_wizard/bB_shadow.png",
    "pieces/gold_wizard/bR_shadow.png",
    "pieces/gold_wizard/bQ_shadow.png",
    "pieces/gold_wizard/bK_shadow.png",
);
/// Ледяной маг (с 0.98, `tools/breach_wizard_combo.py`) — тела, потом тени.
const WIZARD_ICE: [&[u8]; 24] = breach!(
    "pieces/wizard_ice/wP.png",
    "pieces/wizard_ice/wN.png",
    "pieces/wizard_ice/wB.png",
    "pieces/wizard_ice/wR.png",
    "pieces/wizard_ice/wQ.png",
    "pieces/wizard_ice/wK.png",
    "pieces/wizard_ice/bP.png",
    "pieces/wizard_ice/bN.png",
    "pieces/wizard_ice/bB.png",
    "pieces/wizard_ice/bR.png",
    "pieces/wizard_ice/bQ.png",
    "pieces/wizard_ice/bK.png",
    "pieces/wizard_ice/wP_shadow.png",
    "pieces/wizard_ice/wN_shadow.png",
    "pieces/wizard_ice/wB_shadow.png",
    "pieces/wizard_ice/wR_shadow.png",
    "pieces/wizard_ice/wQ_shadow.png",
    "pieces/wizard_ice/wK_shadow.png",
    "pieces/wizard_ice/bP_shadow.png",
    "pieces/wizard_ice/bN_shadow.png",
    "pieces/wizard_ice/bB_shadow.png",
    "pieces/wizard_ice/bR_shadow.png",
    "pieces/wizard_ice/bQ_shadow.png",
    "pieces/wizard_ice/bK_shadow.png",
);

/// Свой спрайт объединённой фигуры: золотой варвар — 0, ледяной — 1, варвар-маг — 2, ледяная
/// золотая — 3, золотой маг — 4, ледяной маг — 5.
fn combo_set(piece: Piece) -> Option<usize> {
    if piece.barbarian {
        [piece.gold, piece.icy, piece.wizard]
            .iter()
            .position(|&b| b)
    } else if piece.gold {
        [piece.icy, piece.wizard]
            .iter()
            .position(|&b| b)
            .map(|i| 3 + i)
    } else if piece.wizard && piece.icy {
        Some(5)
    } else {
        None
    }
}

/// Король-толстосум rogue-like (с 0.73) — `tools/breach_fat_king.py`: белые, чёрные.
const FAT_KING: [&[u8]; 4] = breach!(
    "pieces/fat_king/wK.png",
    "pieces/fat_king/bK.png",
    "pieces/fat_king/wK_shadow.png",
    "pieces/fat_king/bK_shadow.png",
);
/// Кромка инея (`ICE[0]` генератора): белая полоса, ползущая вверх по фигуре.
const FROST_EDGE: [u8; 3] = [0xf2, 0xfb, 0xff];
const SHADOWS: [&[u8]; 12] = breach!(
    "pieces/wP_shadow.png",
    "pieces/wN_shadow.png",
    "pieces/wB_shadow.png",
    "pieces/wR_shadow.png",
    "pieces/wQ_shadow.png",
    "pieces/wK_shadow.png",
    "pieces/bP_shadow.png",
    "pieces/bN_shadow.png",
    "pieces/bB_shadow.png",
    "pieces/bR_shadow.png",
    "pieces/bQ_shadow.png",
    "pieces/bK_shadow.png",
);
/// Клетки: по четыре варианта тёмных (трава) и светлых (песок).
const TILES: [&[u8]; 8] = breach!(
    "tiles/dark_0.png",
    "tiles/dark_1.png",
    "tiles/dark_2.png",
    "tiles/dark_3.png",
    "tiles/light_0.png",
    "tiles/light_1.png",
    "tiles/light_2.png",
    "tiles/light_3.png",
);
/// Курсоры в порядке `Pointer` (рисует `tools/breach_cursor.py`).
/// Курсор удара (`cursor_attack.png`) с 0.32 не нужен: ходом не бьют.
const CURSORS: [&[u8]; 5] = breach!(
    "cursor/cursor_default.png",
    "cursor/cursor_hover.png",
    "cursor/cursor_move.png",
    "cursor/cursor_card.png",
    "cursor/cursor_off.png",
);
/// Холст курсора — стрелка-дротик с тенью внутри; точка нажатия — кончик стрелки.
pub const CURSOR: Vec2 = vec2(22.0, 24.0);
pub const CURSOR_HOTSPOT: Vec2 = vec2(1.0, 1.0);

/// Вид курсора в игре.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pointer {
    /// Обычная стрелка.
    Default,
    /// Над своей фигурой, которую можно выбрать: кромка цвета выделения.
    Hover,
    /// Сюда можно сходить: голубой ромб-клетка.
    Move,

    /// Карту можно взять и перетащить: мини-карта.
    Card,
    /// Нельзя: серый ромб, перечёркнутый красным.
    Off,
}

mod cards;
mod ui;

pub use cards::{
    BURN_CARD_AT, BURN_FRAME, BURN_FRAME_TIME, BURN_FRAMES, CARD, COST_TINT, CardPic, DECK_SKINS,
    EMBLEM_ZOOM, POINTS_TINT, SHADOW_SHIFT, Sprite, cost_corner,
};
pub use ui::{
    ARROW_HEAD, ARROW_SPACING, Aim, ArrowPart, BIG_CHEST, BIG_WAVE_CENTER, BIG_WAVE_FRAMES,
    BLESS_ANCHOR, BLESS_FIGURE, BLESS_FRAME_TIME, BLESS_FRAMES, BUTTON, Button, CHEST, COIN,
    COIN_SMALL, CRATER_CENTER, CRATER_FRAME_TIME, CRATER_FRAMES, CREST, DARK_LOOP, ELECTRIC_ANCHOR,
    ELECTRIC_FLASH, ELECTRIC_FRAME_TIME, ELECTRIC_LOOP, ELECTRIC_SLOW, FREEZE_CENTER, FREEZE_CLINK,
    FREEZE_DELAY, FREEZE_FRAME_TIME, FREEZE_FRAMES, FREEZE_FROST, FrameSprite, LAVA_FRAME_TIME,
    LAVA_FRAMES, LAVA_HOLD, LAVA_IGNITE, LAVA_SPREAD, LAVA_WAVE_AT, MEDAL, METEOR_CENTER,
    METEOR_FRAME_TIME, METEOR_FRAMES, METEOR_IMPACT, METEOR_WAVE_FRAMES, POISON_LOOP,
    POISON_TRAIL_CENTER, POISON_TRAIL_FRAMES, PORTAL_CENTER, PORTAL_CUT, PORTAL_FRAME,
    PORTAL_FRAME_TIME, PORTAL_FRAMES, PORTAL_RADII, PORTAL_SCALE, PORTAL_SINK, POWER_CARD,
    STEP_FRAME_TIME, STEP_IN, STEP_IN_AT, STEP_OUT, TARGET_PAD, WAVE_CENTER, WAVE_FRAME,
    WAVE_FRAME_TIME, WAVE_FRAMES, WIND_LOOP,
};

/// Значки в панелях увеличены вдвое: так они остаются резкими и в крупной карточке фигуры.
const ICON_SCALE: usize = 2;

fn index(color: Color, kind: Kind) -> usize {
    color as usize * 6 + kind as usize
}

/// Хитбокс фигуры: непрозрачные пиксели, у которых всё вокруг на `HIT_SHRINK` тоже
/// непрозрачно; у короля — только ниже `KING_HIT_TOP`.
fn hitbox(body: &Pixels, king: bool) -> Vec<bool> {
    let r = HIT_SHRINK as isize;
    let solid = |x: isize, y: isize| {
        (0..body.width as isize).contains(&x)
            && (0..body.height as isize).contains(&y)
            && body.alpha(x as usize, y as usize) > 0
    };
    let top = if king { KING_HIT_TOP } else { 0 };
    (0..body.height)
        .flat_map(|y| (0..body.width).map(move |x| (x, y)))
        .map(|(x, y)| {
            y >= top
                && (-r..=r).all(|dy| (-r..=r).all(|dx| solid(x as isize + dx, y as isize + dy)))
        })
        .collect()
}

/// Подсветка клетки: полупрозрачный ромб с пиксельной рамкой, как в Into the Breach.
#[derive(Clone, Copy)]
pub enum Marker {
    /// Выбранная фигура.
    Selected,
    /// Откуда и куда пошла фигура последним ходом.
    LastMove,
    /// Ход на пустую клетку.
    Move,
    /// Удар.
    Attack,
    /// Клетка под курсором, куда можно пойти или ударить (поверх хода или удара).
    Target,
    /// Шах: короля могут убить.
    Check,
    /// Угроза королю.
    Threat,
    /// Сплошной белый ромб — заливка клетки любым цветом.
    Fill,
    /// Клетка, на которую ляжет лёд карты (буря, лавина, ледяной барьер).
    Ice,
}

impl Marker {
    const ALL: [Marker; 9] = [
        Marker::Selected,
        Marker::LastMove,
        Marker::Move,
        Marker::Attack,
        Marker::Target,
        Marker::Check,
        Marker::Threat,
        Marker::Fill,
        Marker::Ice,
    ];

    fn image(self) -> Pixels {
        // Заливка, рамка, непрозрачность заливки, отступ от края ромба и толщина рамки.
        let (fill, line, alpha, inset, width) = match self {
            Marker::Selected => (0xffe27a, 0xfff3b8, 70, 1.0, 2.0),
            Marker::LastMove => (0xffe27a, 0xffe27a, 45, 3.0, 1.0),
            // Плотнее, чем на превью генератора: там ходы были только по траве, а на
            // песке синий при 95 выходит серым.
            Marker::Move => (0x2f8fe0, 0x7fd4ff, 125, 2.0, 2.0),
            Marker::Attack => (0xe0452f, 0xff9a6b, 110, 2.0, 2.0),
            // С 0.58 (просьба пользователя) — толстая рамка и светлая заливка: тонкую линию на
            // синих клетках не было видно.
            Marker::Target => (0xffffff, 0xffffff, 110, 0.0, 3.0),
            Marker::Check => (0xff2a1a, 0xffb08a, 140, 1.0, 2.0),
            Marker::Threat => (0xe68a2e, 0xffc35a, 100, 2.0, 2.0),
            Marker::Fill => (0xffffff, 0xffffff, 255, 0.0, 0.0),
            // Цвета льда из breach_freeze.py: заливка `ICE[2]`, кромка `ICE[0]`.
            Marker::Ice => (0x8fcbe6, 0xf2fbff, 120, 1.0, 1.0),
        };
        overlay(rgb(fill), rgb(line), alpha, inset, width)
    }
}

const fn rgb(hex: u32) -> [u8; 3] {
    [(hex >> 16) as u8, (hex >> 8) as u8, hex as u8]
}

/// Картинка с обычной (не умноженной на прозрачность) альфой.
#[derive(Clone)]
struct Pixels {
    width: usize,
    height: usize,
    rgba: Vec<[u8; 4]>,
}

impl Pixels {
    fn new(width: usize, height: usize) -> Pixels {
        Pixels {
            width,
            height,
            rgba: vec![[0; 4]; width * height],
        }
    }

    fn decode(png: &[u8]) -> Pixels {
        let pixmap = tiny_skia::Pixmap::decode_png(png).expect("встроенная картинка не читается");
        let rgba = pixmap
            .pixels()
            .iter()
            .map(|pixel| {
                let c = pixel.demultiply();
                [c.red(), c.green(), c.blue(), c.alpha()]
            })
            .collect();
        Pixels {
            width: pixmap.width() as usize,
            height: pixmap.height() as usize,
            rgba,
        }
    }

    fn alpha(&self, x: usize, y: usize) -> u8 {
        self.rgba[y * self.width + x][3]
    }

    fn set(&mut self, x: usize, y: usize, rgb: [u8; 3], alpha: u8) {
        self.rgba[y * self.width + x] = [rgb[0], rgb[1], rgb[2], alpha];
    }

    /// Кладёт `top` левым верхним углом в точку (`x0`, `y0`), смешивая по прозрачности.
    fn paste(&mut self, top: &Pixels, x0: i32, y0: i32) {
        for y in 0..top.height {
            for x in 0..top.width {
                let (tx, ty) = (x0 + x as i32, y0 + y as i32);
                if tx < 0 || ty < 0 || tx as usize >= self.width || ty as usize >= self.height {
                    continue;
                }
                let src = top.rgba[y * top.width + x];
                let dst = &mut self.rgba[ty as usize * self.width + tx as usize];
                let (sa, da) = (src[3] as f32 / 255.0, dst[3] as f32 / 255.0);
                let out = sa + da * (1.0 - sa);
                if out <= 0.0 {
                    continue;
                }
                for c in 0..3 {
                    let mixed = (src[c] as f32 * sa + dst[c] as f32 * da * (1.0 - sa)) / out;
                    dst[c] = mixed.round() as u8;
                }
                dst[3] = (out * 255.0).round() as u8;
            }
        }
    }

    /// Границы непрозрачной части: левый верхний угол и размер.
    fn bounds(&self) -> ([usize; 2], [usize; 2]) {
        let (mut min, mut max) = ([self.width, self.height], [0, 0]);
        for y in 0..self.height {
            for x in 0..self.width {
                if self.alpha(x, y) > 0 {
                    min = [min[0].min(x), min[1].min(y)];
                    max = [max[0].max(x + 1), max[1].max(y + 1)];
                }
            }
        }
        if max[0] <= min[0] {
            return ([0, 0], [self.width, self.height]);
        }
        (min, [max[0] - min[0], max[1] - min[1]])
    }

    fn crop(&self, min: [usize; 2], size: [usize; 2]) -> Pixels {
        let mut out = Pixels::new(size[0], size[1]);
        for y in 0..size[1] {
            for x in 0..size[0] {
                out.rgba[y * size[0] + x] = self.rgba[(min[1] + y) * self.width + min[0] + x];
            }
        }
        out
    }

    /// Увеличение в `k` раз без сглаживания: пиксели арта остаются квадратными, а
    /// текстура при выводе лишь немного уменьшается — края пикселей чёткие.
    fn upscale(&self, k: usize) -> ColorImage {
        // Цвет умножается на прозрачность прямо в sRGB, как смешивает и генератор арта:
        // `from_rgba_unmultiplied` делает это в линейном пространстве, и полупрозрачные
        // подсветки выходят заметно светлее.
        let premultiplied: Vec<[u8; 4]> = self
            .rgba
            .iter()
            .map(|&[r, g, b, a]| {
                let mul = |c: u8| ((c as u32 * a as u32 + 127) / 255) as u8;
                [mul(r), mul(g), mul(b), a]
            })
            .collect();
        let (width, height) = (self.width * k, self.height * k);
        let mut bytes = Vec::with_capacity(width * height * 4);
        for y in 0..height {
            let row = &premultiplied[(y / k) * self.width..][..self.width];
            for x in 0..width {
                bytes.extend_from_slice(&row[x / k]);
            }
        }
        ColorImage::from_rgba_premultiplied([width, height], &bytes)
    }
}

/// Кромка инея на фигуре `body` цветом `FROST_EDGE`: её светлые (не контурные) пиксели, а с
/// `crest` — только те из них, над которыми прозрачно (верхние края силуэта).
fn frost_edge(body: &Pixels, crest: bool) -> Pixels {
    let mut edge = Pixels::new(body.width, body.height);
    for y in 0..body.height {
        for x in 0..body.width {
            let [r, g, b, a] = body.rgba[y * body.width + x];
            let light = a > 0 && r as u32 + g as u32 + b as u32 >= 60;
            let open = y == 0 || body.alpha(x, y - 1) == 0;
            if light && (!crest || open) {
                edge.set(x, y, FROST_EDGE, 255);
            }
        }
    }
    edge
}

/// Ромб размером с верхнюю грань клетки, ужатый на `inset` пикселей.
fn diamond_mask(inset: f32) -> Vec<bool> {
    let (w, h) = (TILE.x, TILE.y);
    let mut mask = Vec::with_capacity((w * h) as usize);
    for y in 0..h as usize {
        for x in 0..w as usize {
            let dx = (x as f32 + 0.5 - w / 2.0).abs() / (w / 2.0 - inset * 4.0 / 3.0);
            let dy = (y as f32 + 0.5 - h / 2.0).abs() / (h / 2.0 - inset);
            mask.push(dx + dy <= 1.0);
        }
    }
    mask
}

fn overlay(fill: [u8; 3], line: [u8; 3], alpha: u8, inset: f32, width: f32) -> Pixels {
    let outer = diamond_mask(inset);
    let inner = diamond_mask(inset + width);
    let mut image = Pixels::new(TILE.x as usize, TILE.y as usize);
    for (i, (&o, &n)) in outer.iter().zip(&inner).enumerate() {
        let (x, y) = (i % image.width, i / image.width);
        if o && !n {
            image.set(x, y, line, 255);
        } else if o {
            image.set(x, y, fill, alpha);
        }
    }
    image
}

/// Пиксельный шрифт: буквы и цифры 6×8 с толстыми линиями, как подписи доски, и
/// значки для чисел над фигурами. `x` — череп, `<` — стрелка ответного удара, `!` — меч,
/// `h` — сердце, `c` — карта с ромбом, как на рубашке колоды, `z` — молния (заряд), `p` — пешка
/// (ходы фигурами), `*` — знак умножения и `.` — точка (мульти, с 0.119).
const GLYPHS: [(u8, [&str; 8]); 29] = [
    (
        b'*',
        [
            "......", "##..##", ".####.", "..##..", ".####.", "##..##", "......", "......",
        ],
    ),
    (b'.', ["..", "..", "..", "..", "..", "..", "##", "##"]),
    (
        b'p',
        [
            "..###..", ".#####.", ".#####.", "..###..", "..###..", ".#####.", "#######", "#######",
        ],
    ),
    (
        b'z',
        [
            "....##", "...##.", "..##..", ".#####", "#####.", "..##..", ".##...", "##....",
        ],
    ),
    (
        b'A',
        [
            ".####.", "##..##", "##..##", "##..##", "######", "##..##", "##..##", "##..##",
        ],
    ),
    (
        b'B',
        [
            "#####.", "##..##", "##..##", "#####.", "##..##", "##..##", "##..##", "#####.",
        ],
    ),
    (
        b'C',
        [
            ".####.", "##..##", "##....", "##....", "##....", "##....", "##..##", ".####.",
        ],
    ),
    (
        b'D',
        [
            "#####.", "##..##", "##..##", "##..##", "##..##", "##..##", "##..##", "#####.",
        ],
    ),
    (
        b'E',
        [
            "######", "##....", "##....", "#####.", "##....", "##....", "##....", "######",
        ],
    ),
    (
        b'F',
        [
            "######", "##....", "##....", "#####.", "##....", "##....", "##....", "##....",
        ],
    ),
    (
        b'G',
        [
            ".####.", "##..##", "##....", "##....", "##.###", "##..##", "##..##", ".####.",
        ],
    ),
    (
        b'H',
        [
            "##..##", "##..##", "##..##", "######", "##..##", "##..##", "##..##", "##..##",
        ],
    ),
    (
        b'0',
        [
            ".####.", "##..##", "##..##", "##..##", "##..##", "##..##", "##..##", ".####.",
        ],
    ),
    (
        b'1',
        [
            "..##..", ".###..", "####..", "..##..", "..##..", "..##..", "..##..", "######",
        ],
    ),
    (
        b'2',
        [
            ".####.", "##..##", "....##", "...##.", "..##..", ".##...", "##....", "######",
        ],
    ),
    (
        b'3',
        [
            ".####.", "##..##", "....##", "..###.", "....##", "....##", "##..##", ".####.",
        ],
    ),
    (
        b'4',
        [
            "...##.", "..###.", ".####.", "##.##.", "######", "...##.", "...##.", "...##.",
        ],
    ),
    (
        b'5',
        [
            "######", "##....", "##....", "#####.", "....##", "....##", "##..##", ".####.",
        ],
    ),
    (
        b'6',
        [
            ".####.", "##....", "##....", "#####.", "##..##", "##..##", "##..##", ".####.",
        ],
    ),
    (
        b'7',
        [
            "######", "....##", "...##.", "...##.", "..##..", "..##..", ".##...", ".##...",
        ],
    ),
    (
        b'8',
        [
            ".####.", "##..##", "##..##", ".####.", "##..##", "##..##", "##..##", ".####.",
        ],
    ),
    (
        b'9',
        [
            ".####.", "##..##", "##..##", "##..##", ".#####", "....##", "....##", ".####.",
        ],
    ),
    (
        b'-',
        [
            ".....", ".....", ".....", "#####", "#####", ".....", ".....", ".....",
        ],
    ),
    (
        b'+',
        [
            "......", "..##..", "..##..", "######", "######", "..##..", "..##..", "......",
        ],
    ),
    (
        b'x',
        [
            ".#####.", "#######", "#..#..#", "#..#..#", "#######", ".#####.", ".#.#.#.", ".......",
        ],
    ),
    (
        b'<',
        [
            "...#...", "..##...", ".######", "#######", ".######", "..##...", "...#...", ".......",
        ],
    ),
    (
        b'!',
        [
            "......##", ".....###", "....###.", ".#.###..", "..###...", "..##....", ".#..#...",
            "#.......",
        ],
    ),
    (
        b'h',
        [
            ".##.##.", "#######", "#######", "#######", ".#####.", "..###..", "...#...", ".......",
        ],
    ),
    (
        b'c',
        [
            "#######", "###.###", "##...##", "#.....#", "##...##", "###.###", "#######", "#######",
        ],
    ),
];

/// Знак с тёмным контуром и тенью снизу; нижний пиксель каждой линии темнее.
/// Картинка на пиксель шире знака с каждой стороны, сверху — на пиксель, снизу — на два.
fn glyph(ch: u8, light: u32, shade: u32) -> Pixels {
    let rows = GLYPHS
        .iter()
        .find(|(c, _)| *c == ch)
        .map(|(_, rows)| rows)
        .expect("знак есть в шрифте");
    let (w, h) = (rows[0].len() + 2, 11);
    let mut body = vec![false; w * h];
    for (y, row) in rows.iter().enumerate() {
        for (x, c) in row.bytes().enumerate() {
            body[(y + 1) * w + x + 1] = c == b'#';
        }
    }
    let at = |x: i32, y: i32| {
        x >= 0 && y >= 0 && (x as usize) < w && (y as usize) < h && {
            body[y as usize * w + x as usize]
        }
    };
    let mut image = Pixels::new(w, h);
    for y in 0..h as i32 {
        for x in 0..w as i32 {
            let color = if at(x, y) {
                if at(x, y + 1) { light } else { shade }
            } else if [(0, 1), (1, 0), (0, -1), (-1, 0), (1, 1), (1, -1), (2, 0)]
                .iter()
                .any(|&(dy, dx)| at(x - dx, y - dy))
            {
                0x08090d
            } else {
                continue;
            };
            image.set(x as usize, y as usize, rgb(color), 255);
        }
    }
    image
}

/// Знак пиксельного шрифта для чисел над фигурами.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Glyph {
    Digit(u8),
    Minus,
    Plus,
    Skull,

    /// Атака.
    Sword,
    /// Здоровье.
    Heart,
    /// Карты в колоде.
    Card,
    /// Заряд электрической фигуры (герб красной молнии).
    Bolt,
    /// Ходы фигурами (rogue-like, над «Конец хода»).
    Pawn,
    /// Знак умножения и точка — мульти (с 0.119).
    Times,
    Dot,
}

/// Знаки шрифта в порядке текстур.
const FONT: &[u8] = b"0123456789-+x<!hczp*.";

impl Glyph {
    fn char(self) -> u8 {
        match self {
            Glyph::Digit(d) => b'0' + d.min(9),
            Glyph::Minus => b'-',
            Glyph::Plus => b'+',
            Glyph::Skull => b'x',

            Glyph::Sword => b'!',
            Glyph::Heart => b'h',
            Glyph::Card => b'c',
            Glyph::Bolt => b'z',
            Glyph::Pawn => b'p',
            Glyph::Times => b'*',
            Glyph::Dot => b'.',
        }
    }

    fn index(self) -> usize {
        FONT.iter()
            .position(|&c| c == self.char())
            .expect("знак есть в шрифте")
    }

    /// Знаки строки из цифр, «+», «-», «×» и «.» (прочие пропускаются).
    pub fn text(text: &str) -> Vec<Glyph> {
        text.chars()
            .filter_map(|c| match c {
                '0'..='9' => Some(Glyph::Digit(c as u8 - b'0')),
                '+' => Some(Glyph::Plus),
                '-' => Some(Glyph::Minus),
                '×' => Some(Glyph::Times),
                '.' => Some(Glyph::Dot),
                _ => None,
            })
            .collect()
    }

    /// Цифры неотрицательного числа.
    pub fn digits(n: i32) -> Vec<Glyph> {
        n.max(0)
            .to_string()
            .bytes()
            .map(|b| Glyph::Digit(b - b'0'))
            .collect()
    }
}

/// Высота знака шрифта (без контура), пиксели.
pub const GLYPH_HEIGHT: f32 = 8.0;
/// Знаки шрифта увеличены ещё вдвое против доски: всплывая, числа подрастают.
const FONT_SCALE: usize = 2;

/// Левый верхний угол блока клетки (`i`, `j`) на картинке доски: `i` растёт вправо-вниз,
/// `j` — влево-вниз, клетка (0, 0) — верхняя.
fn cell_origin(i: usize, j: usize) -> (i32, i32) {
    let half = (TILE.x as i32 / 2, TILE.y as i32 / 2);
    (
        (i as i32 - j as i32) * half.0 + (BOARD.x as i32 - TILE.x as i32) / 2,
        (i + j) as i32 * half.1,
    )
}

/// Какая клетка шахмат (вертикаль, горизонталь) стоит на месте (`i`, `j`). Белые — внизу
/// слева, а у перевёрнутой доски — чёрные.
fn square_of(i: usize, j: usize, flipped: bool) -> (usize, usize) {
    if flipped { (7 - i, j) } else { (i, 7 - j) }
}

/// Доска целиком: клетки от дальних к ближним (передние закрывают бока задних), буквы
/// под левым передним краем, цифры — под правым.
fn board_image(tiles: &[Pixels], flipped: bool) -> Pixels {
    let mut image = Pixels::new(BOARD.x as usize, BOARD.y as usize);
    let mut cells: Vec<(usize, usize)> = (0..8).flat_map(|i| (0..8).map(move |j| (i, j))).collect();
    cells.sort_by_key(|&(i, j)| i + j);
    for (i, j) in cells {
        let (file, rank) = square_of(i, j, flipped);
        let light = (file + rank) % 2 == 1;
        let variant = (file * 7 + rank * 3) % 4;
        let (x, y) = cell_origin(i, j);
        image.paste(&tiles[light as usize * 4 + variant], x, y);
    }
    for k in 0..8 {
        let (file, _) = square_of(k, 7, flipped);
        let (x, y) = cell_origin(k, 7);
        image.paste(&glyph(b"ABCDEFGH"[file], 0xe9ecf2, 0xa9b0bf), x + 9, y + 61);
        let (_, rank) = square_of(7, k, flipped);
        let (x, y) = cell_origin(7, k);
        image.paste(
            &glyph(b"12345678"[rank], 0xe9ecf2, 0xa9b0bf),
            x + 39,
            y + 61,
        );
    }
    image
}

/// Текстуры доски, фигур и подсветок, увеличенные в `scale` раз.
pub struct Textures {
    scale: u32,
    boards: [TextureHandle; 2],
    tiles: Vec<TextureHandle>,
    /// Белый пиксель: бока клеток, тени и подсветки красятся вершинами.
    solid: TextureHandle,
    bodies: Vec<TextureHandle>,
    gold_bodies: Vec<TextureHandle>,
    gold_shadows: Vec<TextureHandle>,
    ice_bodies: Vec<TextureHandle>,
    ice_shadows: Vec<TextureHandle>,
    poison_bodies: Vec<TextureHandle>,
    poison_shadows: Vec<TextureHandle>,
    barbarian_bodies: Vec<TextureHandle>,
    barbarian_shadows: Vec<TextureHandle>,
    wizard_bodies: Vec<TextureHandle>,
    wizard_shadows: Vec<TextureHandle>,
    armor_bodies: Vec<TextureHandle>,
    armor_shadows: Vec<TextureHandle>,
    /// Свои спрайты объединённых (`combo_set`): [набор][тела, потом тени].
    combo_sprites: [Vec<TextureHandle>; 6],
    vampire_bodies: Vec<TextureHandle>,
    vampire_shadows: Vec<TextureHandle>,
    zombie_bodies: Vec<TextureHandle>,
    zombie_shadows: Vec<TextureHandle>,
    poison_zombie_bodies: Vec<TextureHandle>,
    poison_zombie_shadows: Vec<TextureHandle>,
    armor_vampire_bodies: Vec<TextureHandle>,
    armor_vampire_shadows: Vec<TextureHandle>,
    /// Король-толстосум: тела белого и чёрного, потом их тени.
    fat_king: Vec<TextureHandle>,
    frozen: Vec<TextureHandle>,
    frost_rims: Vec<TextureHandle>,
    frost_crests: Vec<TextureHandle>,
    shadows: Vec<TextureHandle>,
    markers: Vec<TextureHandle>,
    font: Vec<TextureHandle>,
    pub cards: cards::CardTextures,
    pub ui: ui::UiTextures,
}

impl Textures {
    pub fn board(&self, flipped: bool) -> TextureId {
        self.boards[flipped as usize].id()
    }

    /// Верх клетки: `light` — песок, иначе трава; `variant` — 0..4, как у плоской доски.
    pub fn tile(&self, light: bool, variant: usize) -> TextureId {
        self.tiles[usize::from(light) * 4 + variant % 4].id()
    }

    pub fn solid(&self) -> TextureId {
        self.solid.id()
    }

    pub fn body(&self, color: Color, kind: Kind) -> TextureId {
        self.bodies[index(color, kind)].id()
    }

    pub fn shadow(&self, color: Color, kind: Kind) -> TextureId {
        self.shadows[index(color, kind)].id()
    }

    /// Спрайт фигуры: золотая (rogue-like) — из золотого набора.
    pub fn piece_body(&self, piece: Piece) -> TextureId {
        if let Some(set) = combo_set(piece) {
            return self.combo_sprites[set][index(piece.color, piece.kind)].id();
        }
        if piece.boss {
            self.fat_king[piece.color as usize].id()
        } else if piece.gold {
            self.gold_bodies[index(piece.color, piece.kind)].id()
        } else if piece.icy {
            self.ice_bodies[index(piece.color, piece.kind)].id()
        } else if piece.venom && piece.zombie {
            self.poison_zombie_bodies[index(piece.color, piece.kind)].id()
        } else if piece.venom {
            self.poison_bodies[index(piece.color, piece.kind)].id()
        } else if piece.barbarian {
            self.barbarian_bodies[index(piece.color, piece.kind)].id()
        } else if piece.wizard {
            self.wizard_bodies[index(piece.color, piece.kind)].id()
        } else if piece.armored && piece.vampire {
            self.armor_vampire_bodies[index(piece.color, piece.kind)].id()
        } else if piece.armored {
            self.armor_bodies[index(piece.color, piece.kind)].id()
        } else if piece.vampire {
            self.vampire_bodies[index(piece.color, piece.kind)].id()
        } else if piece.zombie {
            self.zombie_bodies[index(piece.color, piece.kind)].id()
        } else {
            self.body(piece.color, piece.kind)
        }
    }

    pub fn piece_shadow(&self, piece: Piece) -> TextureId {
        if let Some(set) = combo_set(piece) {
            return self.combo_sprites[set][12 + index(piece.color, piece.kind)].id();
        }
        if piece.boss {
            self.fat_king[2 + piece.color as usize].id()
        } else if piece.gold {
            self.gold_shadows[index(piece.color, piece.kind)].id()
        } else if piece.icy {
            self.ice_shadows[index(piece.color, piece.kind)].id()
        } else if piece.venom && piece.zombie {
            self.poison_zombie_shadows[index(piece.color, piece.kind)].id()
        } else if piece.venom {
            self.poison_shadows[index(piece.color, piece.kind)].id()
        } else if piece.barbarian {
            self.barbarian_shadows[index(piece.color, piece.kind)].id()
        } else if piece.wizard {
            self.wizard_shadows[index(piece.color, piece.kind)].id()
        } else if piece.armored && piece.vampire {
            self.armor_vampire_shadows[index(piece.color, piece.kind)].id()
        } else if piece.armored {
            self.armor_shadows[index(piece.color, piece.kind)].id()
        } else if piece.vampire {
            self.vampire_shadows[index(piece.color, piece.kind)].id()
        } else if piece.zombie {
            self.zombie_shadows[index(piece.color, piece.kind)].id()
        } else {
            self.shadow(piece.color, piece.kind)
        }
    }

    /// Фигура в инее — на том же холсте, что и обычная.
    pub fn frozen(&self, color: Color, kind: Kind) -> TextureId {
        self.frozen[index(color, kind)].id()
    }

    /// Кромка инея, пока он поднимается по фигуре: `crest` — верхние края силуэта (белые
    /// пиксели под прозрачными), иначе — все светлые пиксели фигуры (для строки самой кромки).
    pub fn frost_edge(&self, color: Color, kind: Kind, crest: bool) -> TextureId {
        let edges = if crest {
            &self.frost_crests
        } else {
            &self.frost_rims
        };
        edges[index(color, kind)].id()
    }

    pub fn marker(&self, marker: Marker) -> TextureId {
        self.markers[marker as usize].id()
    }

    /// Знак шрифта: белый с тёмным контуром, красится оттенком при выводе.
    pub fn glyph(&self, glyph: Glyph) -> TextureId {
        self.font[glyph.index()].id()
    }
}

pub struct Art {
    bodies: Vec<Pixels>,
    gold_bodies: Vec<Pixels>,
    gold_shadows: Vec<Pixels>,
    ice_bodies: Vec<Pixels>,
    ice_shadows: Vec<Pixels>,
    poison_bodies: Vec<Pixels>,
    poison_shadows: Vec<Pixels>,
    barbarian_bodies: Vec<Pixels>,
    barbarian_shadows: Vec<Pixels>,
    wizard_bodies: Vec<Pixels>,
    wizard_shadows: Vec<Pixels>,
    armor_bodies: Vec<Pixels>,
    armor_shadows: Vec<Pixels>,
    combo_sprites: [Vec<Pixels>; 6],
    vampire_bodies: Vec<Pixels>,
    vampire_shadows: Vec<Pixels>,
    zombie_bodies: Vec<Pixels>,
    zombie_shadows: Vec<Pixels>,
    poison_zombie_bodies: Vec<Pixels>,
    poison_zombie_shadows: Vec<Pixels>,
    armor_vampire_bodies: Vec<Pixels>,
    armor_vampire_shadows: Vec<Pixels>,
    fat_king: Vec<Pixels>,
    frozen: Vec<Pixels>,
    /// Кромка инея: светлые пиксели фигуры и верхние края её силуэта (`frost_reveal` в
    /// breach_freeze.py).
    frost_rims: Vec<Pixels>,
    frost_crests: Vec<Pixels>,
    shadows: Vec<Pixels>,
    /// Хитбоксы фигур (`hitbox`): какие пиксели холста ловят курсор.
    hitboxes: Vec<Vec<bool>>,
    boards: [Pixels; 2],
    /// Клетки по отдельности: четыре тёмных варианта, потом четыре светлых.
    /// Плоская доска собирается из них и забывает, а осмотр кладёт каждую на свой блок.
    tiles: Vec<Pixels>,
    markers: Vec<Pixels>,
    font: Vec<Pixels>,
    cards: cards::CardArt,
    ui: ui::UiArt,
    cursors: Vec<Pixels>,
    /// Курсоры при одном увеличении: оно своё, целое, чтобы стрелка была резкой.
    cursor_textures: RefCell<Option<(u32, Vec<TextureHandle>)>>,
    /// Первая непрозрачная строка каждой фигуры — над ней ставятся числа.
    tops: Vec<usize>,
    textures: RefCell<Option<Rc<Textures>>>,
    icons: RefCell<Vec<(TextureHandle, Vec2)>>,
}

impl Art {
    pub fn new() -> Art {
        let bodies: Vec<Pixels> = BODIES.iter().map(|png| Pixels::decode(png)).collect();
        let tiles: Vec<Pixels> = TILES.iter().map(|png| Pixels::decode(png)).collect();
        let boards = [board_image(&tiles, false), board_image(&tiles, true)];
        Art {
            tops: bodies.iter().map(|body| body.bounds().0[1]).collect(),
            frozen: FROZEN.iter().map(|png| Pixels::decode(png)).collect(),
            frost_rims: bodies.iter().map(|body| frost_edge(body, false)).collect(),
            frost_crests: bodies.iter().map(|body| frost_edge(body, true)).collect(),
            shadows: SHADOWS.iter().map(|png| Pixels::decode(png)).collect(),
            gold_bodies: GOLD_BODIES.iter().map(|png| Pixels::decode(png)).collect(),
            gold_shadows: GOLD_SHADOWS.iter().map(|png| Pixels::decode(png)).collect(),
            ice_bodies: ICE_BODIES.iter().map(|png| Pixels::decode(png)).collect(),
            ice_shadows: ICE_SHADOWS.iter().map(|png| Pixels::decode(png)).collect(),
            poison_bodies: POISON_BODIES
                .iter()
                .map(|png| Pixels::decode(png))
                .collect(),
            poison_shadows: POISON_SHADOWS
                .iter()
                .map(|png| Pixels::decode(png))
                .collect(),
            barbarian_bodies: BARBARIAN_BODIES
                .iter()
                .map(|png| Pixels::decode(png))
                .collect(),
            barbarian_shadows: BARBARIAN_SHADOWS
                .iter()
                .map(|png| Pixels::decode(png))
                .collect(),
            fat_king: FAT_KING.iter().map(|png| Pixels::decode(png)).collect(),
            armor_bodies: ARMOR_BODIES.iter().map(|png| Pixels::decode(png)).collect(),
            combo_sprites: [
                &BARBARIAN_GOLD,
                &BARBARIAN_ICE,
                &WIZARD_HORNS,
                &GOLD_ICE,
                &GOLD_WIZARD,
                &WIZARD_ICE,
            ]
            .map(|set| set.iter().map(|png| Pixels::decode(png)).collect()),
            armor_shadows: ARMOR_SHADOWS
                .iter()
                .map(|png| Pixels::decode(png))
                .collect(),
            vampire_bodies: VAMPIRE_BODIES
                .iter()
                .map(|png| Pixels::decode(png))
                .collect(),
            vampire_shadows: VAMPIRE_SHADOWS
                .iter()
                .map(|png| Pixels::decode(png))
                .collect(),
            zombie_bodies: ZOMBIE_BODIES
                .iter()
                .map(|png| Pixels::decode(png))
                .collect(),
            zombie_shadows: ZOMBIE_SHADOWS
                .iter()
                .map(|png| Pixels::decode(png))
                .collect(),
            poison_zombie_bodies: POISON_ZOMBIE_BODIES
                .iter()
                .map(|png| Pixels::decode(png))
                .collect(),
            poison_zombie_shadows: POISON_ZOMBIE_SHADOWS
                .iter()
                .map(|png| Pixels::decode(png))
                .collect(),
            armor_vampire_bodies: ARMOR_VAMPIRE_BODIES
                .iter()
                .map(|png| Pixels::decode(png))
                .collect(),
            armor_vampire_shadows: ARMOR_VAMPIRE_SHADOWS
                .iter()
                .map(|png| Pixels::decode(png))
                .collect(),
            wizard_bodies: WIZARD_BODIES
                .iter()
                .map(|png| Pixels::decode(png))
                .collect(),
            wizard_shadows: WIZARD_SHADOWS
                .iter()
                .map(|png| Pixels::decode(png))
                .collect(),
            hitboxes: bodies
                .iter()
                .enumerate()
                .map(|(i, body)| hitbox(body, i % 6 == Kind::King as usize))
                .collect(),
            boards,
            tiles,
            markers: Marker::ALL.iter().map(|m| m.image()).collect(),
            font: FONT.iter().map(|&c| glyph(c, 0xffffff, 0xb4b4b4)).collect(),
            cards: cards::CardArt::new(),
            ui: ui::UiArt::new(),
            cursors: CURSORS.iter().map(|png| Pixels::decode(png)).collect(),
            cursor_textures: RefCell::new(None),
            bodies,
            textures: RefCell::new(None),
            icons: RefCell::new(Vec::new()),
        }
    }

    /// Текстуры, где пиксель арта — `scale`×`scale` пикселей.
    pub fn textures(&self, ctx: &egui::Context, scale: u32) -> Rc<Textures> {
        let scale = scale.clamp(1, 8);
        let mut cache = self.textures.borrow_mut();
        if let Some(textures) = cache.as_ref().filter(|t| t.scale == scale) {
            return Rc::clone(textures);
        }
        let k = scale as usize;
        let load = |name: String, image: &Pixels| {
            ctx.load_texture(name, image.upscale(k), TextureOptions::LINEAR)
        };
        let all = |name: &str, images: &[Pixels]| -> Vec<TextureHandle> {
            images
                .iter()
                .enumerate()
                .map(|(i, image)| load(format!("{name}-{i}-x{scale}"), image))
                .collect()
        };
        let textures = Rc::new(Textures {
            scale,
            boards: [
                load(format!("board-x{scale}"), &self.boards[0]),
                load(format!("board-flipped-x{scale}"), &self.boards[1]),
            ],
            tiles: all("tile", &self.tiles),
            solid: ctx.load_texture(
                format!("solid-x{scale}"),
                ColorImage::new([1, 1], egui::Color32::WHITE),
                TextureOptions::NEAREST,
            ),
            bodies: all("piece", &self.bodies),
            gold_bodies: all("gold-piece", &self.gold_bodies),
            gold_shadows: all("gold-shadow", &self.gold_shadows),
            ice_bodies: all("ice-piece", &self.ice_bodies),
            ice_shadows: all("ice-shadow", &self.ice_shadows),
            poison_bodies: all("poison-piece", &self.poison_bodies),
            poison_shadows: all("poison-shadow", &self.poison_shadows),
            barbarian_bodies: all("barbarian-piece", &self.barbarian_bodies),
            barbarian_shadows: all("barbarian-shadow", &self.barbarian_shadows),
            wizard_bodies: all("wizard-piece", &self.wizard_bodies),
            wizard_shadows: all("wizard-shadow", &self.wizard_shadows),
            armor_bodies: all("armor-piece", &self.armor_bodies),
            combo_sprites: [0, 1, 2, 3, 4, 5]
                .map(|i| all(&format!("combo-{i}"), &self.combo_sprites[i])),
            armor_shadows: all("armor-shadow", &self.armor_shadows),
            vampire_bodies: all("vampire-piece", &self.vampire_bodies),
            vampire_shadows: all("vampire-shadow", &self.vampire_shadows),
            zombie_bodies: all("zombie-piece", &self.zombie_bodies),
            zombie_shadows: all("zombie-shadow", &self.zombie_shadows),
            poison_zombie_bodies: all("poison-zombie-piece", &self.poison_zombie_bodies),
            poison_zombie_shadows: all("poison-zombie-shadow", &self.poison_zombie_shadows),
            armor_vampire_bodies: all("armor-vampire-piece", &self.armor_vampire_bodies),
            armor_vampire_shadows: all("armor-vampire-shadow", &self.armor_vampire_shadows),
            fat_king: all("fat-king", &self.fat_king),
            frozen: all("frozen-piece", &self.frozen),
            frost_rims: all("frost-rim", &self.frost_rims),
            frost_crests: all("frost-crest", &self.frost_crests),
            shadows: all("shadow", &self.shadows),
            markers: all("marker", &self.markers),
            font: self
                .font
                .iter()
                .enumerate()
                .map(|(i, image)| {
                    let image = image.upscale(k * FONT_SCALE);
                    ctx.load_texture(format!("glyph-{i}-x{scale}"), image, TextureOptions::LINEAR)
                })
                .collect(),
            cards: self.cards.textures(ctx, k),
            ui: self.ui.textures(ctx, k),
        });
        *cache = Some(Rc::clone(&textures));
        textures
    }

    /// Высота макушки фигуры над точкой, стоящей в центре клетки, в пикселях арта.
    pub fn height(&self, color: Color, kind: Kind) -> f32 {
        ANCHOR.y - self.tops[index(color, kind)] as f32
    }

    /// Цены карт в балансе изменились: перерисовывает лица карт (и сбрасывает текстуры).
    pub fn sync_costs(&mut self) {
        if self.cards.sync_costs() {
            *self.textures.borrow_mut() = None;
        }
    }

    /// Картинка карты, повёрнутая на `degrees` по часовой стрелке.
    pub fn card(
        &self,
        ctx: &egui::Context,
        textures: &Textures,
        pic: CardPic,
        degrees: f32,
    ) -> Sprite {
        self.cards.turned(ctx, &textures.cards, pic, degrees)
    }

    /// Лицо карты для показа в `zoom` раз крупнее — карта под курсором.
    pub fn card_zoomed(
        &self,
        ctx: &egui::Context,
        textures: &Textures,
        pic: CardPic,
        zoom: usize,
    ) -> Sprite {
        self.cards.zoomed(ctx, &textures.cards, pic, zoom)
    }

    /// Ширина знака шрифта без контура, пиксели.
    pub fn glyph_width(&self, glyph: Glyph) -> f32 {
        (self.font[glyph.index()].width - 2) as f32
    }

    /// Закрывает ли фигура точку `at` своего холста (в пикселях арта).
    pub fn covers(&self, color: Color, kind: Kind, at: Vec2) -> bool {
        if at.x < 0.0 || at.y < 0.0 || at.x >= SPRITE.x || at.y >= SPRITE.y {
            return false;
        }
        self.bodies[index(color, kind)].alpha(at.x as usize, at.y as usize) > 0
    }

    /// Ловит ли фигура курсор в точке `at` своего холста (в пикселях арта) — по хитбоксу.
    pub fn hits(&self, color: Color, kind: Kind, at: Vec2) -> bool {
        if at.x < 0.0 || at.y < 0.0 || at.x >= SPRITE.x || at.y >= SPRITE.y {
            return false;
        }
        self.hitboxes[index(color, kind)][at.y as usize * SPRITE.x as usize + at.x as usize]
    }

    /// Часть стрелки прицеливания, повёрнутая на `degrees` (кратно 5) по часовой стрелке.
    pub fn arrow(
        &self,
        ctx: &egui::Context,
        textures: &Textures,
        part: ArrowPart,
        aim: Aim,
        degrees: i32,
    ) -> Sprite {
        self.ui.arrow(ctx, &textures.ui, part, aim, degrees)
    }

    /// Курсор, увеличенный ровно в `scale` раз и выводимый без сглаживания.
    pub fn cursor(&self, ctx: &egui::Context, pointer: Pointer, scale: u32) -> TextureId {
        let scale = scale.clamp(1, 8);
        let mut cache = self.cursor_textures.borrow_mut();
        if cache.as_ref().is_none_or(|(s, _)| *s != scale) {
            let textures = self
                .cursors
                .iter()
                .enumerate()
                .map(|(i, image)| {
                    let image = image.upscale(scale as usize);
                    ctx.load_texture(
                        format!("cursor-{i}-x{scale}"),
                        image,
                        TextureOptions::NEAREST,
                    )
                })
                .collect();
            *cache = Some((scale, textures));
        }
        let (_, textures) = cache.as_ref().expect("курсоры только что загружены");
        textures[pointer as usize].id()
    }

    /// Значок фигуры для панелей: текстура обрезанной по краям фигуры и её размер в
    /// пикселях арта.
    pub fn icon(&self, ctx: &egui::Context, color: Color, kind: Kind) -> (TextureId, Vec2) {
        let mut icons = self.icons.borrow_mut();
        if icons.is_empty() {
            let options = TextureOptions::LINEAR.with_mipmap_mode(Some(TextureFilter::Linear));
            *icons = self
                .bodies
                .iter()
                .enumerate()
                .map(|(i, body)| {
                    let (min, size) = body.bounds();
                    let image = body.crop(min, size).upscale(ICON_SCALE);
                    let texture = ctx.load_texture(format!("icon-{i}"), image, options);
                    (texture, vec2(size[0] as f32, size[1] as f32))
                })
                .collect();
        }
        let (texture, size) = &icons[index(color, kind)];
        (texture.id(), *size)
    }
}

/// Значок окна — белый конь cburnett с lichess.
pub fn window_icon() -> IconData {
    const SIZE: u32 = 64;
    let svg = include_bytes!("../assets/piece/cburnett/wN.svg");
    let tree = usvg::Tree::from_data(svg, &usvg::Options::default())
        .expect("встроенный SVG фигуры не читается");
    let mut pixmap = tiny_skia::Pixmap::new(SIZE, SIZE).expect("размер значка не нулевой");
    let scale = SIZE as f32 / tree.size().width().max(tree.size().height());
    resvg::render(
        &tree,
        tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    let rgba = pixmap
        .pixels()
        .iter()
        .flat_map(|pixel| {
            let c = pixel.demultiply();
            [c.red(), c.green(), c.blue(), c.alpha()]
        })
        .collect();
    IconData {
        rgba,
        width: SIZE,
        height: SIZE,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursors_match_the_generator() {
        for png in CURSORS {
            let image = Pixels::decode(png);
            assert_eq!(
                [image.width, image.height],
                [CURSOR.x as usize, CURSOR.y as usize]
            );
            // Кончик стрелки непрозрачен, правый верхний угол пуст.
            let (x, y) = (CURSOR_HOTSPOT.x as usize, CURSOR_HOTSPOT.y as usize);
            assert_eq!(image.alpha(x, y), 255);
            assert_eq!(image.alpha(image.width - 1, 0), 0);
        }
    }

    #[test]
    fn hitboxes_are_inside_the_sprites_and_the_king_is_cut() {
        for (i, png) in BODIES.iter().enumerate() {
            let body = Pixels::decode(png);
            let king = i % 6 == Kind::King as usize;
            let hit = hitbox(&body, king);
            let at = |x: usize, y: usize| y * body.width + x;
            let (inside, all) = (0..body.height)
                .flat_map(|y| (0..body.width).map(move |x| (x, y)))
                .fold((0, 0), |(inside, all), (x, y)| {
                    let solid = body.alpha(x, y) > 0;
                    assert!(!hit[at(x, y)] || solid, "хитбокс — внутри силуэта");
                    (
                        inside + usize::from(hit[at(x, y)]),
                        all + usize::from(solid),
                    )
                });
            assert!(inside > all / 3 && inside < all, "{i}: {inside} из {all}");
            // Основание фигуры ловит курсор.
            assert!(hit[at(ANCHOR.x as usize, ANCHOR.y as usize)], "{i}");
            if king {
                let top = (0..body.height).find(|&y| (0..body.width).any(|x| hit[at(x, y)]));
                assert!(top >= Some(KING_HIT_TOP), "клетка позади короля свободна");
            }
        }
    }

    #[test]
    fn sprites_fit_the_board_layout() {
        for png in BODIES
            .iter()
            .chain(&SHADOWS)
            .chain(&GOLD_BODIES)
            .chain(&GOLD_SHADOWS)
            .chain(&ICE_BODIES)
            .chain(&ICE_SHADOWS)
            .chain(&POISON_BODIES)
            .chain(&POISON_SHADOWS)
            .chain(&BARBARIAN_BODIES)
            .chain(&BARBARIAN_SHADOWS)
            .chain(&WIZARD_BODIES)
            .chain(&WIZARD_SHADOWS)
            .chain(&ARMOR_BODIES)
            .chain(&ARMOR_SHADOWS)
            .chain(&VAMPIRE_BODIES)
            .chain(&VAMPIRE_SHADOWS)
            .chain(&ZOMBIE_BODIES)
            .chain(&ZOMBIE_SHADOWS)
            .chain(&POISON_ZOMBIE_BODIES)
            .chain(&POISON_ZOMBIE_SHADOWS)
            .chain(&ARMOR_VAMPIRE_BODIES)
            .chain(&ARMOR_VAMPIRE_SHADOWS)
            .chain(&BARBARIAN_GOLD)
            .chain(&BARBARIAN_ICE)
            .chain(&WIZARD_HORNS)
            .chain(&GOLD_ICE)
            .chain(&GOLD_WIZARD)
            .chain(&WIZARD_ICE)
            .chain(&FAT_KING)
        {
            let image = Pixels::decode(png);
            assert_eq!(
                [image.width, image.height],
                [SPRITE.x as usize, SPRITE.y as usize]
            );
        }
        for png in TILES {
            let image = Pixels::decode(png);
            assert_eq!(image.width, TILE.x as usize);
            // Нижняя клетка доски доходит ровно до низа картинки доски.
            let (_, y) = cell_origin(7, 7);
            assert_eq!(y as usize + image.height, BOARD.y as usize);
        }
    }

    #[test]
    fn board_is_lit_where_the_cells_are() {
        let art = Art::new();
        for board in &art.boards {
            // Центр каждой клетки непрозрачен, углы картинки — пустые.
            for i in 0..8 {
                for j in 0..8 {
                    let (x, y) = cell_origin(i, j);
                    let (cx, cy) = (x as usize + 28, y as usize + 21);
                    assert_eq!(board.alpha(cx, cy), 255);
                }
            }
            assert_eq!(board.alpha(0, 0), 0);
            assert_eq!(board.alpha(BOARD.x as usize - 1, 0), 0);
        }
        // Внизу слева у белых — a1, тёмная клетка; у перевёрнутой доски там h8, тоже тёмная.
        assert_eq!(square_of(0, 7, false), (0, 0));
        assert_eq!(square_of(0, 7, true), (7, 7));
    }
}
